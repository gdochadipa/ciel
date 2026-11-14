"""Check the SDK pack output and the protected fixture agree on its location."""
from pathlib import Path
import re
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import service_check_linux


class ReleaseFixturePaths(unittest.TestCase):
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
