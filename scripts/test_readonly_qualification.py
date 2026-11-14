import importlib.util
import json
from pathlib import Path
import sys
import unittest

spec = importlib.util.spec_from_file_location(
    'readonly_qualification', Path(__file__).with_name('readonly-qualification.py'))
qualification = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qualification)


class ReadOnlyQualificationTests(unittest.TestCase):
    def bundle(self, label='operator_live_core'):
        body = '{"ok":true,"data":{"text":"é"}}'
        data = body.encode()
        return {
            'run': {
                'schema': 'ciel.readonly.run.v1',
                'evidence_class': label,
                'observations': [
                    {'response_sha256': qualification.sha(data), 'response_bytes': len(data)}
                    for _ in range(4)
                ],
            },
            'captures': [{'response': body} for _ in range(4)],
        }

    def test_review_checks_exact_unicode_bytes_and_denies_incomplete_or_tampered_capture(self):
        value = self.bundle()
        self.assertEqual(qualification.verify_capture(qualification.encode(value)), value['run'])
        value['captures'][0]['response'] += ' '
        with self.assertRaises(qualification.Denied):
            qualification.verify_capture(qualification.encode(value))
        value['captures'].pop()
        with self.assertRaises(qualification.Denied):
            qualification.verify_capture(qualification.encode(value))

    def test_review_cannot_promote_local_simulation_or_duplicate_keys(self):
        with self.assertRaises(qualification.Denied):
            qualification.verify_capture(json.dumps(self.bundle('local_simulation')).encode())
        with self.assertRaises(qualification.Denied):
            qualification.exact_json(b'{"binding":"first","binding":"second"}')

    def test_supervisor_reaps_timeout_and_rejects_nonzero_exit(self):
        with self.assertRaises(qualification.Denied):
            qualification.supervise([sys.executable, '-c', 'import time; time.sleep(20)'], 0.2)
        with self.assertRaises(qualification.Denied):
            qualification.supervise([sys.executable, '-c', 'raise SystemExit(130)'], 5)
        pid, output = qualification.supervise([sys.executable, '-c', 'print("fixture")'], 5)
        self.assertGreater(pid, 0)
        self.assertEqual(output, b'fixture\n')


if __name__ == '__main__':
    unittest.main()
