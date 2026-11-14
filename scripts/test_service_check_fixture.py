"""Check protected release fixture paths and timeout supervision."""
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import service_check_linux


class ReleaseFixturePaths(unittest.TestCase):
    def test_readonly_timeout_is_owned_after_the_credential_drop(self):
        fixture = service_check_linux.ServiceCheckLinux()
        fixture.manifest_path = Path('/synthetic/manifest.json')
        fixture.probe_arguments = lambda path: [
            '/synthetic/setpriv', '--reuid=1', '--regid=1', 'probe', str(path)]
        with patch.object(service_check_linux.subprocess, 'run') as run:
            run.return_value = subprocess.CompletedProcess([], 1, b'', b'')
            result = fixture.readonly_command('/synthetic/core', 'check-release-readonly', '/synthetic/plugin')
        arguments, keywords = run.call_args
        command = arguments[0]
        self.assertNotIn('timeout', keywords, 'Root without CAP_KILL cannot own a dropped-UID timeout')
        self.assertEqual(command[:3], ['/synthetic/setpriv', '--reuid=1', '--regid=1'])
        self.assertEqual(command[3], sys.executable)
        self.assertEqual(result.returncode, 1)

    def test_readonly_timeout_does_not_need_the_parent_to_signal_its_child(self):
        with tempfile.TemporaryDirectory(prefix='ciel-timeout-fixture-') as directory:
            executable = Path(directory) / 'slow-core'
            executable.write_text('#!' + sys.executable + '\nimport time\ntime.sleep(0.3)\n')
            executable.chmod(0o700)
            fixture = service_check_linux.ServiceCheckLinux()
            fixture.manifest_path = Path(directory) / 'manifest.json'
            # This portable case starts already under the test user's identity.
            # Only the Linux suite proves the actual setpriv/capability boundary.
            fixture.probe_arguments = lambda path: ['probe', str(path)]
            actual_run = subprocess.run

            def short_run(*args, **kwargs):
                if 'timeout' in kwargs:
                    kwargs['timeout'] = 0.05
                return actual_run(*args, **kwargs)

            with patch.object(service_check_linux, 'READONLY_COMMAND_TIMEOUT', 0.05, create=True), \
                    patch.object(service_check_linux.subprocess, 'run', side_effect=short_run), \
                    patch.object(subprocess.Popen, 'kill', side_effect=PermissionError(1, 'Synthetic cross-UID signal denial')):
                with self.assertRaises(subprocess.TimeoutExpired) as raised:
                    fixture.readonly_command(executable, 'check-release-readonly', directory)
            self.assertEqual(raised.exception.timeout, 0.05)

    def test_readonly_supervisor_preserves_completed_cli_output_status_and_arguments(self):
        with tempfile.TemporaryDirectory(prefix='ciel-output-fixture-') as directory:
            executable = Path(directory) / 'core'
            executable.write_text(
                '#!' + sys.executable + '\nimport os, sys\n'
                'assert not os.environ.get("CIEL_PARENT_ONLY")\n'
                'assert sys.argv[1:3] == ["service", "check-release-readonly"]\n'
                'assert sys.argv[-2:] == ["--synthetic", "exact argument"]\n'
                'sys.stdout.buffer.write(b"synthetic stdout\\n")\n'
                'sys.stderr.buffer.write(b"synthetic stderr\\n")\n'
                'sys.exit(7)\n')
            executable.chmod(0o700)
            fixture = service_check_linux.ServiceCheckLinux()
            fixture.manifest_path = Path(directory) / 'manifest.json'
            fixture.probe_arguments = lambda path: ['probe', str(path)]
            with patch.dict(service_check_linux.os.environ, {'CIEL_PARENT_ONLY': 'not inherited'}):
                result = fixture.readonly_command(executable, 'check-release-readonly', directory,
                                                  trailing=('--synthetic', 'exact argument'))
            self.assertEqual((result.returncode, result.stdout, result.stderr),
                             (7, b'synthetic stdout\n', b'synthetic stderr\n'))

    def test_admission_denial_preserves_the_gateway_canary_and_detects_new_state(self):
        with tempfile.TemporaryDirectory(prefix='ciel-denial-fixture-') as directory:
            root = Path(directory)
            fixture = service_check_linux.ServiceCheckLinux()
            fixture.root = root
            fixture.state = root / 'state'
            fixture.state.mkdir()
            fixture.config = root / 'config'
            fixture.config.mkdir()
            fixture.compatible_release = lambda: (root / 'core', root / 'plugin')
            fixture.write_json = lambda path, value: path.write_text('{}')
            fixture.watch_secret_inodes = lambda: 17

            def gateway_plan():
                (fixture.state / 'private-state').write_bytes(b'synthetic private-state canary')
                return root / 'gateway-plan.json', {}

            fixture.gateway_plan = gateway_plan
            fixture.readonly_command = lambda *args, **kwargs: SimpleNamespace(
                returncode=1, stdout=b'', stderr=b'')
            with patch.object(service_check_linux.os, 'read', side_effect=BlockingIOError):
                fixture.test_admission_denies_missing_promotion_and_invalid_plan_before_secrets()

                def changed_state(*args, **kwargs):
                    (fixture.state / 'unexpected-journal').write_bytes(b'unexpected')
                    return SimpleNamespace(returncode=1, stdout=b'', stderr=b'')

                fixture.readonly_command = changed_state
                with self.assertRaises(AssertionError):
                    fixture.test_admission_denies_missing_promotion_and_invalid_plan_before_secrets()

    def test_actual_compatible_release_helper_consumes_launcher_pack_output(self):
        source = Path(__file__).resolve().parent.parent
        launcher = (source / 'scripts/acceptance-service-check-linux.sh').read_text()
        output = re.search(r'plugins pack .*? --out (/target/[^\s]+)', launcher)
        self.assertIsNotNone(output)
        with tempfile.TemporaryDirectory(prefix='ciel-release-fixture-') as directory:
            root = Path(directory)
            target = root / 'target'
            binary = target / 'debug/secretary-core'
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b'synthetic executable path fixture')
            # Produce only the archive membership used by the real helper.
            # This test checks the path seam, not SDK build/compatibility.
            import tarfile
            import io
            archive = root / output[1].lstrip('/')
            archive.parent.mkdir(parents=True, exist_ok=True)
            with tarfile.open(archive, 'w:gz') as packed:
                for name in ('package.json', 'openclaw.plugin.json', 'dist/index.js'):
                    data = b'synthetic package path fixture'
                    member = tarfile.TarInfo('package/' + name)
                    member.size = len(data)
                    packed.addfile(member, io.BytesIO(data))

            fixture = service_check_linux.ServiceCheckLinux()
            fixture.root = root / 'fixture'
            fixture.binary = str(binary)
            for name in service_check_linux.ARTIFACTS:
                path = fixture.root / 'release' / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'synthetic release path fixture')
            fixture.write = lambda path, data, mode=0o640: Path(path).write_bytes(data)
            fixture.change_json = lambda field, change: change({})
            with patch.object(service_check_linux.os, 'confstr', return_value='glibc 2.36'):
                installed_core, plugin = fixture.compatible_release()
            self.assertEqual(installed_core.read_bytes(), binary.read_bytes())
            self.assertEqual(
                (fixture.root / 'release/plugins/secretary-tools.tgz').read_bytes(),
                archive.read_bytes(),
            )
            self.assertEqual((plugin / 'dist/index.js').read_bytes(), b'synthetic package path fixture')


if __name__ == '__main__':
    unittest.main()
