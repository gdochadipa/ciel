"""Exercise operator CLI boundaries with disposable local files, never live secrets."""
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent


class OperatorCommands(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='secretary-operations-')
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.state = self.directory / 'state'
        self.state.mkdir(mode=0o700)
        self.registry = self.write('registry.json', '{}')
        self.credentials = self.write('credentials.json', json.dumps({'type': 'service_account'}))
        self.qualification = {
            'qualified': True, 'openclaw_version': '2026.10.7',
            'evidence': {key: 'Synthetic operator attestation, not live evidence' for key in (
                'runtime_activation', 'owner_and_outbound_policy', 'google_disposable_sheet',
                'trusted_full_diff_approval', 'native_scheduler_restart_and_delivery',
                'explicit_memory_policy', 'backup_restore')},
        }
        self.record = self.write('qualification.json', json.dumps(self.qualification))
        self.values = {
            'SECRETARY_CONFIG': str(self.registry),
            'GOOGLE_APPLICATION_CREDENTIALS': str(self.credentials),
            'SECRETARY_STATE_DIR': str(self.state),
            'SECRETARY_CORE_TOKEN': 'synthetic-core-' + 'a' * 40,
            'SECRETARY_APPROVAL_TOKEN': 'synthetic-approval-' + 'b' * 40,
            'SECRETARY_OWNER': '+15555550123',
            'SECRETARY_MOCK_MODE': 'false', 'SECRETARY_WRITES_ENABLED': 'false',
        }
        self.environment = self.write('service.env', '')

    def write(self, name, contents):
        path = self.directory / name
        path.write_text(contents)
        path.chmod(0o600)
        return path

    def run_command(self, script, *args):
        return subprocess.run([sys.executable, str(ROOT / script), *map(str, args)],
                              text=True, capture_output=True, timeout=15)

    def preflight(self, extra=''):
        self.environment.write_text(''.join(f'{key}={value}\n' for key, value in self.values.items()) + extra)
        return self.run_command('preflight.py', '--env-file', self.environment,
                                '--qualification-file', self.record)

    def assert_denied(self, result):
        self.assertEqual(result.returncode, 1)
        self.assertIn('Preflight denied:', result.stderr)
        output = result.stdout + result.stderr
        for value in (self.values['SECRETARY_CORE_TOKEN'], self.values['SECRETARY_APPROVAL_TOKEN'],
                      str(self.directory), 'SYNTHETIC_PRIVATE_DETAIL'):
            self.assertNotIn(value, output)

    @unittest.skipIf(os.geteuid() == 0, 'successful service preflight requires a non-root service user')
    def test_complete_private_operator_attestation_passes_local_checks_only(self):
        result = self.preflight()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('operator qualification and local file checks only', result.stdout)

    def test_missing_unqualified_wrong_version_and_incomplete_evidence_are_denied(self):
        for change in ({'qualified': False}, {'openclaw_version': 'other'}, {'evidence': {}},
                       {'evidence': {'runtime_activation': 'SYNTHETIC_PRIVATE_DETAIL'}}):
            with self.subTest(change=change):
                self.record.write_text(json.dumps({**self.qualification, **change}))
                self.assert_denied(self.preflight())
        self.record.unlink()
        self.assert_denied(self.preflight())

    def test_malformed_or_non_object_qualification_fails_without_private_output(self):
        for text in ('{SYNTHETIC_PRIVATE_DETAIL', '[]', 'null', '"SYNTHETIC_PRIVATE_DETAIL"'):
            with self.subTest(text=text):
                self.record.write_text(text)
                self.assert_denied(self.preflight())

    def test_public_files_and_symlinked_configuration_are_denied(self):
        for path in (self.record, self.environment, self.registry, self.credentials):
            with self.subTest(path=path.name):
                path.chmod(0o644)
                self.assert_denied(self.preflight())
                path.chmod(0o600)
                original = path.with_suffix('.private')
                path.rename(original)
                path.symlink_to(original)
                self.assert_denied(self.preflight())
                path.unlink()
                original.rename(path)

    def test_public_or_symlinked_state_directory_is_denied(self):
        self.state.chmod(0o755)
        self.assert_denied(self.preflight())
        self.state.chmod(0o700)
        alias = self.directory / 'state-alias'
        alias.symlink_to(self.state, target_is_directory=True)
        self.values['SECRETARY_STATE_DIR'] = str(alias)
        self.assert_denied(self.preflight())

    def test_mock_mode_writes_and_unsafe_tokens_are_denied(self):
        changes = [('SECRETARY_MOCK_MODE', 'true'), ('SECRETARY_WRITES_ENABLED', 'true'),
                   ('SECRETARY_CORE_TOKEN', 'short'),
                   ('SECRETARY_APPROVAL_TOKEN', self.values['SECRETARY_CORE_TOKEN']),
                   ('SECRETARY_CORE_TOKEN', 'REPLACE' + 'x' * 40)]
        for key, value in changes:
            with self.subTest(key=key, value=value):
                original = self.values[key]
                self.values[key] = value
                self.assert_denied(self.preflight())
                self.values[key] = original

    def test_duplicate_and_unsupported_environment_entries_are_denied(self):
        for extra in ('SECRETARY_OWNER=SYNTHETIC_PRIVATE_DETAIL\n', 'not an entry\n',
                      'export SECRETARY_OWNER=SYNTHETIC_PRIVATE_DETAIL\n'):
            with self.subTest(extra=extra):
                self.assert_denied(self.preflight(extra))

    def test_invalid_registry_and_credential_documents_are_denied(self):
        for path, text in ((self.registry, '[]'), (self.credentials, '{SYNTHETIC_PRIVATE_DETAIL'),
                           (self.credentials, '[]'), (self.credentials, '{}')):
            with self.subTest(path=path.name, text=text):
                original = path.read_text()
                path.write_text(text)
                self.assert_denied(self.preflight())
                path.write_text(original)

    @unittest.skipIf(os.geteuid() == 0, 'capability preflight requires a non-root service user')
    def test_readable_docs_require_separate_evidence_even_when_writes_are_disabled(self):
        self.registry.write_text(json.dumps({'documents': [
            {'alias': 'notes', 'id': 'synthetic-doc', 'readable': True}]}))
        self.assert_denied(self.preflight())
        for evidence in (None, '', '  ', False, [], {}):
            with self.subTest(evidence=evidence):
                self.record.write_text(json.dumps({**self.qualification, 'evidence': {
                    **self.qualification['evidence'], 'google_disposable_docs': evidence}}))
                self.assert_denied(self.preflight())
        self.record.write_text(json.dumps({**self.qualification, 'evidence': {
            **self.qualification['evidence'],
            'google_disposable_docs': 'Synthetic Docs API/ACL/tab/revision evidence; not live'}}))
        result = self.preflight()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('operator qualification and local file checks only', result.stdout)

    @unittest.skipIf(os.geteuid() == 0, 'capability preflight requires a non-root service user')
    def test_unreadable_docs_keep_sheets_only_attestation_compatible(self):
        self.registry.write_text(json.dumps({'documents': [
            {'alias': 'notes', 'id': 'synthetic-doc', 'readable': False}]}))
        self.assertEqual(self.preflight().returncode, 0)

    def test_malformed_document_capabilities_cannot_bypass_evidence_gate(self):
        invalid = [None, {}, 'SYNTHETIC_PRIVATE_DETAIL', [None], [True], [{}]]
        invalid.extend([[{'readable': value}] for value in (None, 'true', 'false', 0, 1)])
        invalid.extend([[{'readable': False, 'writable': value}]
                        for value in (True, 'false', 0, None)])
        for documents in invalid:
            with self.subTest(documents=documents):
                self.registry.write_text(json.dumps({'documents': documents}))
                self.assert_denied(self.preflight())

    def test_backup_includes_committed_wal_and_excludes_uncommitted_changes(self):
        source = self.state / 'journal.sqlite3'
        live = sqlite3.connect(source)
        self.addCleanup(live.close)
        live.execute('PRAGMA journal_mode=WAL')
        live.execute('PRAGMA wal_autocheckpoint=0')
        live.execute('CREATE TABLE operations (id TEXT PRIMARY KEY, state TEXT)')
        live.execute('INSERT INTO operations VALUES (?, ?)', ('synthetic-op', 'applying'))
        live.commit()
        self.assertGreater(Path(str(source) + '-wal').stat().st_size, 0)
        live.execute('UPDATE operations SET state=?', ('uncommitted',))
        snapshot = self.directory / 'snapshot.sqlite3'
        result = self.run_command('backup.py', source, snapshot)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(snapshot.stat().st_mode & 0o777, 0o600)
        with sqlite3.connect(snapshot) as restored:
            self.assertEqual(restored.execute('PRAGMA journal_mode').fetchone(), ('delete',))
            self.assertEqual(restored.execute('PRAGMA integrity_check').fetchone(), ('ok',))
            self.assertEqual(restored.execute('SELECT state FROM operations').fetchone(), ('applying',))
        original = snapshot.read_bytes()
        self.assertEqual(self.run_command('backup.py', source, snapshot).returncode, 1)
        self.assertEqual(snapshot.read_bytes(), original)
        self.assertEqual(live.execute('SELECT state FROM operations').fetchone(), ('uncommitted',))
        live.rollback()
        self.assertEqual(live.execute('SELECT state FROM operations').fetchone(), ('applying',))
        self.assertEqual(list(self.directory.glob('.secretary-backup-*')), [])

    def test_backup_rejects_public_destination_corrupt_or_missing_source(self):
        corrupt = self.write('corrupt.sqlite3', 'SYNTHETIC_PRIVATE_DETAIL')
        destination = self.directory / 'failed.sqlite3'
        for source in (corrupt, self.directory / 'missing.sqlite3'):
            result = self.run_command('backup.py', source, destination)
            self.assertEqual(result.returncode, 1)
            self.assertNotIn('SYNTHETIC_PRIVATE_DETAIL', result.stdout + result.stderr)
            self.assertFalse(destination.exists())
            self.assertEqual(list(self.directory.glob('.secretary-backup-*')), [])
        public = self.directory / 'public'
        public.mkdir(mode=0o755)
        result = self.run_command('backup.py', corrupt, public / 'snapshot.sqlite3')
        self.assertEqual(result.returncode, 1)
        self.assertEqual(list(public.iterdir()), [])


if __name__ == '__main__':
    unittest.main()
