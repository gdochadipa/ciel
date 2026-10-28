"""Public launcher behavior with disposable external command runners."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent


class AcceptanceLauncher(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='secretary-launcher-')
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.source = self.directory / 'source'
        scripts = self.source / 'scripts'
        scripts.mkdir(parents=True)
        shutil.copy2(ROOT / 'scripts/acceptance.sh', scripts)
        diagnostics = ROOT / 'scripts/acceptance_diagnostics.py'
        if diagnostics.exists():
            shutil.copy2(diagnostics, scripts)
        self.commands = self.directory / 'bin'
        self.commands.mkdir()
        self.logs = self.directory / 'logs'
        self.logs.mkdir()
        self.summary = self.directory / 'job-summary'
        self.environment = dict(os.environ, PATH=str(self.commands) + os.pathsep + os.environ['PATH'],
                                SECRETARY_ACCEPTANCE_LOG_ROOT=str(self.logs),
                                GITHUB_STEP_SUMMARY=str(self.summary))
        self.command('cargo', 'printf \'{"reason":"compiler-artifact"}\\n\'')
        self.command('bun', 'printf "/bin/false\\n"')

    def command(self, name, body):
        path = self.commands / name
        path.write_text('#!/bin/sh\n' + body + '\n')
        path.chmod(0o755)

    def run_launcher(self):
        return subprocess.run(['bash', str(self.source / 'scripts/acceptance.sh'), 'read'],
                              env=self.environment, text=True, capture_output=True, timeout=20)

    def diagnostic(self):
        paths = list(self.logs.glob('secretary-acceptance.*/diagnostic.json'))
        self.assertEqual(len(paths), 1)
        return json.loads(paths[0].read_text())

    def test_startup_stdout_error_is_classified_without_printing_provider_content(self):
        self.command('postman', '''printf '%s\\n' '{"event":"error","message":"EADDRINUSE secret-canary-123","authorization":"Bearer secret-canary-123"}'
exit 42''')
        result = self.run_launcher()
        self.assertNotEqual(result.returncode, 0)
        diagnostic = self.diagnostic()
        self.assertEqual(diagnostic['stage'], 'simulation_start')
        self.assertEqual(diagnostic['simulation_exit_code'], 42)
        self.assertEqual(diagnostic['error_categories'], ['address_in_use'])
        for output in [result.stdout, result.stderr, self.summary.read_text(), json.dumps(diagnostic)]:
            self.assertNotIn('secret-canary-123', output)
            self.assertNotIn('authorization', output)
        self.assertIn('simulation_start', self.summary.read_text())

    def ready_provider(self):
        self.environment['FIXTURE_SIM_PID'] = str(self.directory / 'sim-pid')
        self.environment['FIXTURE_CORE_PID'] = str(self.directory / 'core-pid')
        self.command('postman', '''echo $$ > "$FIXTURE_SIM_PID"
printf '%s\\n' '{"event":"listening","url":"http://127.0.0.1:4900"}' '{"event":"request"}'
exec sleep 60''')
        self.command('fixture-core', '''echo $$ > "$FIXTURE_CORE_PID"
exec sleep 60''')

    def assert_services_stopped(self):
        for name in ['sim-pid', 'core-pid']:
            pid = int((self.directory / name).read_text())
            with self.assertRaises(ProcessLookupError):
                os.kill(pid, 0)

    def test_client_failure_preserves_exit_status_and_cleans_owned_services(self):
        self.ready_provider()
        self.command('bun', '''if [ "$1" = -e ]; then command -v fixture-core
elif [ "$2" = --simulation-base ]; then echo 'http://127.0.0.1:4900/google/v4'
else
attempt=0
while [ ! -s "$FIXTURE_CORE_PID" ] && [ "$attempt" -lt 100 ]; do sleep 0.01; attempt=$((attempt+1)); done
[ -s "$FIXTURE_CORE_PID" ] || exit 9
echo 'ECONNREFUSED secret-canary-456' >&2; exit 7; fi''')
        result = self.run_launcher()
        self.assertEqual(result.returncode, 7)
        diagnostic = self.diagnostic()
        self.assertEqual(diagnostic['stage'], 'client')
        self.assertEqual(diagnostic['exit_code'], 7)
        self.assertEqual(diagnostic['error_categories'], ['connection_failed'])
        self.assertEqual(diagnostic['event_counts_in_log_tails']['request'], 1)
        self.assertNotIn('secret-canary-456', result.stdout + result.stderr + self.summary.read_text())
        self.assert_services_stopped()

    def test_success_keeps_client_result_and_does_not_report_a_failure(self):
        self.ready_provider()
        self.command('bun', '''if [ "$1" = -e ]; then command -v fixture-core
elif [ "$2" = --simulation-base ]; then echo 'http://127.0.0.1:4900/google/v4'
else
attempt=0
while [ ! -s "$FIXTURE_CORE_PID" ] && [ "$attempt" -lt 100 ]; do sleep 0.01; attempt=$((attempt+1)); done
[ -s "$FIXTURE_CORE_PID" ] || exit 9
echo '{"acceptance":"passed","scenario":"read"}'; fi''')
        result = self.run_launcher()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('"acceptance":"passed"', result.stdout)
        self.assertEqual(list(self.logs.glob('*/diagnostic.json')), [])
        self.assertFalse(self.summary.exists())
        self.assert_services_stopped()


    def test_build_failure_has_stage_and_status_without_echoing_tool_output(self):
        self.command('cargo', "echo 'EACCES secret-build-canary' >&2; exit 23")
        result = self.run_launcher()
        self.assertEqual(result.returncode, 23)
        diagnostic = self.diagnostic()
        self.assertEqual(diagnostic['stage'], 'core_build')
        self.assertEqual(diagnostic['exit_code'], 23)
        self.assertEqual(diagnostic['error_categories'], ['permission_denied'])
        self.assertNotIn('secret-build-canary', result.stdout + result.stderr + self.summary.read_text())


    def test_diagnostic_cli_bounds_malformed_logs_and_exposes_only_allowlisted_metadata(self):
        log = self.logs / 'bounded'
        log.mkdir()
        # An old error outside the retained tail must not be reported.
        malformed = '[' * 4000 + ']' * 4000
        (log / 'sim.ndjson').write_text('EADDRINUSE old-secret\n' + 'x' * 70000 + '\n' +
                                      malformed + '\n' +
                                      '{"event":{"private":"secret-canary"}}\n' +
                                      '{"event":"error","message":"EACCES secret-canary"}\n')
        result = subprocess.run(['python3', str(ROOT / 'scripts/acceptance_diagnostics.py'),
                                 '--log-dir', str(log), '--scenario', 'read', '--stage',
                                 'simulation_start', '--exit-code', '1'],
                                env=self.environment, capture_output=True, text=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        diagnostic = json.loads((log / 'diagnostic.json').read_text())
        self.assertEqual(diagnostic['error_categories'], ['permission_denied'])
        self.assertEqual(diagnostic['event_counts_in_log_tails'],
                         {'listening': 0, 'request': 0, 'error': 1})
        self.assertNotIn('secret-canary', result.stdout + self.summary.read_text())
        self.assertLess(len(result.stdout), 1024)


    def test_parser_failures_keep_raw_stderr_private_and_preserve_status(self):
        for stage in ['core_artifact', 'provider_address']:
            with self.subTest(stage=stage):
                shutil.rmtree(self.logs)
                self.logs.mkdir()
                self.summary.unlink(missing_ok=True)
                self.ready_provider()
                self.environment['FIXTURE_FAIL_STAGE'] = stage
                self.command('bun', '''if [ "$1" = -e ] && [ "$FIXTURE_FAIL_STAGE" != core_artifact ]; then
command -v fixture-core
else echo 'unknown option secret-parser-canary' >&2; exit 9; fi''')
                result = self.run_launcher()
                self.assertEqual(result.returncode, 9)
                diagnostic = self.diagnostic()
                self.assertEqual(diagnostic['stage'], stage)
                self.assertNotIn('secret-parser-canary', result.stdout + result.stderr + self.summary.read_text())
                if stage == 'provider_address':
                    pid = int((self.directory / 'sim-pid').read_text())
                    with self.assertRaises(ProcessLookupError):
                        os.kill(pid, 0)


if __name__ == '__main__':
    unittest.main()
