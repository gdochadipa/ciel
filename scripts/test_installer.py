"""Public installer CLI tests; fake only external build executables, never SDK types."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / 'scripts' / 'install.py'


class InstallerCLI(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='secretary-installer-')
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name).resolve()
        self.environment = dict(os.environ)

    def run_cli(self, *args):
        return subprocess.run([sys.executable, str(SCRIPT), *map(str, args)],
                              capture_output=True, text=True, timeout=30, env=self.environment)

    def source_fixture(self):
        source = self.directory / 'checkout'
        source.mkdir()
        for name in ('.gitignore', 'LICENSE', 'Cargo.toml', 'Cargo.lock', 'bun.lock', 'package.json',
                     'plugins/secretary-tools/package.json', 'plugins/secretary-tools/openclaw.plugin.json',
                     'scripts/preflight.py', 'scripts/backup.py', '.env.example',
                     'config/workspace.example.json', 'config/openclaw.example.json',
                     'deploy/qualification.example.json', 'deploy/secretary-core.service', 'docs/installation.md'):
            target = source / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, target)
        for command in (['git', 'init', '-q'], ['git', 'add', '.'],
                        ['git', '-c', 'user.name=Installer fixture', '-c', 'user.email=fixture@example.invalid',
                         '-c', 'commit.gpgsign=false', '-c', 'core.hooksPath=/dev/null', 'commit', '-qm', 'Synthetic fixture']):
            subprocess.run(command, cwd=source, check=True, capture_output=True)
        revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source, text=True).strip()
        return source, revision

    def test_plan_binds_a_clean_reviewed_commit_without_building_or_writing(self):
        source, revision = self.source_fixture()
        result = self.run_cli('plan', '--source', source, '--revision', revision)
        self.assertEqual(result.returncode, 0, result.stderr)
        plan = json.loads(result.stdout)
        self.assertEqual(plan['source_commit'], revision)
        self.assertFalse(plan['starts_services'])
        self.assertFalse(plan['activates_plugins'])
        self.assertFalse(plan['production_writes'])
        self.assertFalse((source / 'target').exists())
        wrong = self.run_cli('plan', '--source', source, '--revision', '0' * 40)
        self.assertEqual(wrong.returncode, 1)
        (source / 'package.json').write_text('{}')
        dirty = self.run_cli('plan', '--source', source, '--revision', revision)
        self.assertEqual(dirty.returncode, 1)

    def build_tools(self):
        tools = self.directory / 'tools'
        tools.mkdir()
        script = r"""
import json,os,sys,tarfile,io
from pathlib import Path
root=Path(__file__).parent
name=Path(sys.argv[0]).name
args=sys.argv[1:]
with (root/'calls.jsonl').open('a') as out:
 out.write(json.dumps({'tool':name,'args':args,'private_env':any(k.startswith(('SECRETARY_','GOOGLE_')) for k in os.environ)})+'\n')
if (root/'fail').exists() and name=='openclaw' and 'pack' in args:
 print('SYNTHETIC_PRIVATE_BUILD_DETAIL',file=sys.stderr);sys.exit(9)
if name=='node':print('v24.22.1')
elif name=='rustc':print('host: aarch64-unknown-linux-gnu' if '-vV' in args else 'rustc 1.95.0 (synthetic)')
elif name=='cargo':
 if '--version' in args:print('cargo 1.95.0 (synthetic)')
 else:
  binary=Path(os.environ.get('CARGO_TARGET_DIR','target'))/'release/secretary-core'
  binary.parent.mkdir(parents=True,exist_ok=True);binary.write_bytes(b'\x7fELFsynthetic-core');binary.chmod(0o755)
  print(json.dumps({'reason':'compiler-artifact','target':{'name':'secretary-core','kind':['bin']},'executable':str(binary.resolve())}))
elif name=='bun':
 if '--version' in args:print('1.2.21')
 elif args and args[0]=='install':
  sdk=Path('node_modules/openclaw');sdk.mkdir(parents=True,exist_ok=True);(sdk/'package.json').write_text('{"version":"2026.10.7"}')
elif name=='openclaw':
 if '--version' in args:print('OpenClaw 2026.10.7 (synthetic)')
 elif 'pack' in args:
  target=Path(args[args.index('--out')+1]);target.parent.mkdir(parents=True,exist_ok=True)
  manifest=Path('plugins/secretary-tools/openclaw.plugin.json').read_bytes()
  with tarfile.open(target,'w:gz') as archive:
   for path,data in [('package/openclaw.plugin.json',manifest),('package/package.json',b'{"type":"module"}'),('package/dist/index.js',b'// synthetic external build result')]:
    entry=tarfile.TarInfo(path);entry.size=len(data);archive.addfile(entry,io.BytesIO(data))
 else:sys.exit(8)
else:sys.exit(8)
"""
        for name in ('node', 'bun', 'cargo', 'rustc', 'openclaw'):
            executable = tools / name
            executable.write_text('#!' + sys.executable + '\n' + script)
            executable.chmod(0o755)
        self.environment['PATH'] = str(tools) + os.pathsep + self.environment['PATH']
        self.environment['SECRETARY_CORE_TOKEN'] = 'SYNTHETIC_PRIVATE_TOKEN'
        self.environment['GOOGLE_APPLICATION_CREDENTIALS'] = 'SYNTHETIC_PRIVATE_PATH'
        self.environment['CARGO_TARGET_DIR'] = str(self.directory / 'build-target')
        return tools

    def test_staging_builds_a_reviewable_release_with_inactive_templates_and_no_live_commands(self):
        source, revision = self.source_fixture()
        tools = self.build_tools()
        output = self.directory / 'release'
        result = self.run_cli('stage', '--source', source, '--revision', revision, '--output', output)
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads((output / 'release.json').read_text())
        self.assertEqual(receipt['source_commit'], revision)
        self.assertFalse(receipt['production_writes'])
        self.assertIn('Copyright (c) 2026 gdochadipa', (output / 'LICENSE').read_text())
        self.assertTrue((output / 'bin/secretary-core').stat().st_mode & 0o111)
        self.assertIn('SECRETARY_WRITES_ENABLED=false', (output / 'templates/service.env.example').read_text())
        self.assertFalse(json.loads((output / 'templates/qualification.example.json').read_text())['qualified'])
        self.assertFalse(json.loads((output / 'templates/registry.example.json').read_text())['spreadsheets'][0]['create_tabs'])
        verified = self.run_cli('verify', '--release', output, '--revision', revision)
        self.assertEqual(verified.returncode, 0, verified.stderr)
        calls = [json.loads(line) for line in (tools / 'calls.jsonl').read_text().splitlines()]
        self.assertFalse(any(c['private_env'] for c in calls))
        self.assertFalse(any(c['tool'] == 'openclaw' and any(a in ('install', 'reload', 'enable') for a in c['args']) for c in calls))
        before = (tools / 'calls.jsonl').read_text()
        replay = self.run_cli('stage', '--source', source, '--revision', revision, '--output', output)
        self.assertEqual(replay.returncode, 0, replay.stderr)
        self.assertTrue(json.loads(replay.stdout)['reused'])
        self.assertEqual((tools / 'calls.jsonl').read_text(), before)

    def test_file_installation_preserves_config_and_state_and_does_not_enable_the_unit(self):
        source, revision = self.source_fixture()
        tools = self.build_tools()
        release = self.directory / 'release'
        self.assertEqual(self.run_cli('stage', '--source', source, '--revision', revision,
                                      '--output', release).returncode, 0)
        root = self.directory / 'target-root'
        config = root / 'etc/secretary/service.env'
        config.parent.mkdir(parents=True)
        config.write_text('SYNTHETIC_PRIVATE_EXISTING_CONFIG')
        journal = root / 'var/lib/secretary/secretary.sqlite3'
        journal.parent.mkdir(parents=True)
        journal.write_bytes(b'SYNTHETIC_EXISTING_JOURNAL')
        before = (tools / 'calls.jsonl').read_text()
        for reused in (False, True):
            result = self.run_cli('install-files', '--release', release, '--revision', revision, '--root', root)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)['reused'], reused)
            self.assertFalse(json.loads(result.stdout)['activates_plugins'])
            self.assertFalse(json.loads(result.stdout)['starts_services'])
        self.assertEqual((tools / 'calls.jsonl').read_text(), before)
        self.assertEqual(config.read_text(), 'SYNTHETIC_PRIVATE_EXISTING_CONFIG')
        self.assertEqual(journal.read_bytes(), b'SYNTHETIC_EXISTING_JOURNAL')
        self.assertTrue((root / 'opt/secretary/bin/secretary-core').is_file())
        self.assertIn('ExecStartPre=', (root / 'etc/systemd/system/secretary-core.service').read_text())
        self.assertFalse((root / 'etc/systemd/system/multi-user.target.wants').exists())
        verify = self.run_cli('verify', '--release', root / 'opt/secretary', '--revision', revision)
        self.assertEqual(verify.returncode, 0, verify.stderr)

    def test_build_failure_is_redacted_and_does_not_publish_a_partial_release(self):
        source, revision = self.source_fixture()
        tools = self.build_tools()
        (tools / 'fail').write_text('pack')
        output = self.directory / 'failed-release'
        result = self.run_cli('stage', '--source', source, '--revision', revision, '--output', output)
        self.assertEqual(result.returncode, 1)
        self.assertNotIn('SYNTHETIC_PRIVATE', result.stdout + result.stderr)
        self.assertNotIn(str(self.directory), result.stdout + result.stderr)
        self.assertFalse(output.exists())
        self.assertEqual(list(self.directory.glob('.failed-release*')), [])

    def test_wrong_toolchain_public_parent_and_foreign_lock_fail_closed(self):
        source, revision = self.source_fixture()
        tools = self.build_tools()
        node = tools / 'node'
        node.write_text(node.read_text().replace('v24.22.1', 'v24.16.0'))
        output = self.directory / 'release'
        denied = self.run_cli('stage', '--source', source, '--revision', revision, '--output', output)
        self.assertEqual(denied.returncode, 1)
        self.assertFalse(output.exists())
        node.write_text(node.read_text().replace('v24.16.0', 'v24.22.1'))
        public = self.directory / 'public'
        public.mkdir(mode=0o755)
        self.assertEqual(self.run_cli('stage', '--source', source, '--revision', revision,
                                     '--output', public / 'release').returncode, 1)
        lock = self.directory / '.release.lock'
        lock.write_text('another installer owns this lock')
        self.assertEqual(self.run_cli('stage', '--source', source, '--revision', revision,
                                     '--output', output).returncode, 1)
        self.assertEqual(lock.read_text(), 'another installer owns this lock')
        self.assertFalse(output.exists())

    def test_tampered_or_extra_artifacts_cannot_be_verified_or_installed(self):
        source, revision = self.source_fixture()
        self.build_tools()
        release = self.directory / 'release'
        self.assertEqual(self.run_cli('stage', '--source', source, '--revision', revision,
                                     '--output', release).returncode, 0)
        root = self.directory / 'target-root'
        root.mkdir()
        binary = release / 'bin/secretary-core'
        original = binary.read_bytes()
        binary.write_bytes(b'corrupt binary')
        for command in ('verify', 'install-files'):
            result = self.run_cli(command, '--release', release, '--revision', revision, '--root', root)
            self.assertEqual(result.returncode, 1)
        self.assertEqual(list(root.iterdir()), [])
        binary.write_bytes(original)
        extra = release / 'credentials.json'
        extra.write_text('SYNTHETIC_PRIVATE_EXTRA')
        result = self.run_cli('verify', '--release', release, '--revision', revision)
        self.assertEqual(result.returncode, 1)
        self.assertNotIn('SYNTHETIC_PRIVATE', result.stdout + result.stderr)
        extra.unlink()
        link = self.directory / 'linked-release'
        link.symlink_to(release, target_is_directory=True)
        self.assertEqual(self.run_cli('verify', '--release', link, '--revision', revision).returncode, 1)

    def test_existing_unit_and_symlinked_installation_paths_are_preserved(self):
        source, revision = self.source_fixture()
        tools = self.build_tools()
        release = self.directory / 'release'
        self.assertEqual(self.run_cli('stage', '--source', source, '--revision', revision,
                                     '--output', release).returncode, 0)
        root = self.directory / 'target-root'
        unit = root / 'etc/systemd/system/secretary-core.service'
        unit.parent.mkdir(parents=True)
        unit.write_text('operator-managed existing unit')
        result = self.run_cli('install-files', '--release', release, '--revision', revision, '--root', root)
        self.assertEqual(result.returncode, 1)
        self.assertFalse((root / 'opt').exists())
        self.assertEqual(unit.read_text(), 'operator-managed existing unit')
        unit.unlink()
        elsewhere = self.directory / 'elsewhere'
        elsewhere.mkdir()
        (root / 'opt').symlink_to(elsewhere, target_is_directory=True)
        result = self.run_cli('install-files', '--release', release, '--revision', revision, '--root', root)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(list(elsewhere.iterdir()), [])

    def test_vendor_unit_prevents_binary_publication(self):
        source, revision = self.source_fixture()
        self.build_tools()
        release = self.directory / 'release'
        self.assertEqual(self.run_cli('stage', '--source', source, '--revision', revision,
                                     '--output', release).returncode, 0)
        root = self.directory / 'target-root'
        vendor = root / 'usr/lib/systemd/system/secretary-core.service'
        vendor.parent.mkdir(parents=True)
        vendor.write_text('[Service]\nExecStart=/opt/secretary/bin/secretary-core\nRestart=always\n')
        result = self.run_cli('install-files', '--release', release, '--revision', revision, '--root', root)
        self.assertEqual(result.returncode, 1)
        self.assertFalse((root / 'opt/secretary/bin/secretary-core').exists())
        self.assertIn('Restart=always', vendor.read_text())

    def test_cargo_target_configuration_cannot_change_the_native_build_target(self):
        source, revision = self.source_fixture()
        tools = self.build_tools()
        configured = source / '.cargo/config.toml'
        configured.parent.mkdir()
        configured.write_text('[build]\ntarget = "x86_64-unknown-linux-musl"\n')
        subprocess.run(['git', 'add', '.cargo/config.toml'], cwd=source, check=True, capture_output=True)
        subprocess.run(['git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                        '-c', 'commit.gpgsign=false', '-c', 'core.hooksPath=/dev/null', 'commit', '-qm', 'Foreign target'],
                       cwd=source, check=True, capture_output=True)
        revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source, text=True).strip()
        result = self.run_cli('stage', '--source', source, '--revision', revision,
                              '--output', self.directory / 'release')
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [json.loads(line) for line in (tools / 'calls.jsonl').read_text().splitlines()]
        build = next(c for c in calls if c['tool'] == 'cargo' and 'build' in c['args'])
        self.assertIn('--target', build['args'])
        self.assertEqual(build['args'][build['args'].index('--target') + 1], 'aarch64-unknown-linux-gnu')

    def test_help_describes_inert_staging_and_separate_file_installation(self):
        result = self.run_cli('--help')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('stage', result.stdout)
        self.assertIn('install-files', result.stdout)
        self.assertIn('never starts', result.stdout)
        self.assertEqual(list(self.directory.iterdir()), [])


if __name__ == '__main__':
    unittest.main()
