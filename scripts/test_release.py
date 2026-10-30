"""Distribution CLI at the approved disposable-file installer seam."""
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
import install

ROOT = Path(__file__).resolve().parent.parent
REVISION = 'a' * 40


class ReleaseCLI(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.release = self.root / 'release'
        self.release.mkdir()
        for target, source in install.TEMPLATES.items():
            dest = self.release / target
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / source, dest)
        binary = self.release / 'bin/secretary-core'
        binary.parent.mkdir()
        binary.write_bytes(b'\x7fELFsynthetic')
        binary.chmod(0o755)
        plugin = self.release / 'plugins/secretary-tools.tgz'
        plugin.parent.mkdir()
        with tarfile.open(plugin, 'w:gz') as tar:
            for name, data in [('package/package.json', b'{}'),
                               ('package/dist/index.js', b'// synthetic SDK artifact'),
                               ('package/openclaw.plugin.json', (ROOT / 'plugins/secretary-tools/openclaw.plugin.json').read_bytes())]:
                member = tarfile.TarInfo(name)
                member.size = len(data)
                tar.addfile(member, io.BytesIO(data))
        self.receipt = dict(format_version=2, source_commit=REVISION,
                            build_host=dict(system='Linux', machine='x86_64', libc=['glibc', '2.39']),
                            production_writes=False, starts_services=False, activates_plugins=False,
                            artifact_hashes={name: install.fingerprint(self.release / name) for name in install.ARTIFACTS})
        self.write_receipt()
        self.archive = self.root / 'ciel.tar.gz'

    def write_receipt(self):
        (self.release / 'release.json').write_text(json.dumps(self.receipt))

    def bundle(self):
        return subprocess.run([sys.executable, str(ROOT / 'scripts/release.py'),
                               '--release', str(self.release), '--revision', REVISION,
                               '--output', str(self.archive)], capture_output=True, text=True)

    def test_download_contains_standalone_verifier_without_checkout_and_is_exclusive(self):
        result = self.bundle()
        self.assertEqual(result.returncode, 0, result.stderr)
        checksum = self.archive.with_suffix('.gz.sha256').read_text().split()[0]
        self.assertEqual(checksum, hashlib.sha256(self.archive.read_bytes()).hexdigest())
        with tarfile.open(self.archive) as tar:
            self.assertTrue(all(m.isfile() for m in tar.getmembers()))
            for member in tar.getmembers():
                relative = Path(member.name)
                self.assertFalse(relative.is_absolute())
                self.assertNotIn('..', relative.parts)
                target = self.root / 'download' / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(tar.extractfile(member).read())
                target.chmod(member.mode)
        downloaded = self.root / 'download' / ('ciel-' + REVISION)
        verified = subprocess.run([sys.executable, str(downloaded / 'install.py'), 'verify',
                                   '--release', str(downloaded / 'release'), '--revision', REVISION],
                                  capture_output=True, text=True)
        self.assertEqual(verified.returncode, 0, verified.stderr)
        self.assertFalse((downloaded / '.git').exists())
        original = self.archive.read_bytes()
        self.assertNotEqual(self.bundle().returncode, 0)
        self.assertEqual(self.archive.read_bytes(), original)

    def test_wrong_target_tampering_and_extra_files_fail_without_a_download(self):
        self.receipt['build_host']['machine'] = 'aarch64'
        self.write_receipt()
        self.assertNotEqual(self.bundle().returncode, 0)
        self.receipt['build_host']['machine'] = 'x86_64'
        self.write_receipt()
        (self.release / 'LICENSE').write_text('tampered')
        self.assertNotEqual(self.bundle().returncode, 0)
        shutil.copyfile(ROOT / 'LICENSE', self.release / 'LICENSE')
        (self.release / 'private.json').write_text('must never be packaged')
        self.assertNotEqual(self.bundle().returncode, 0)
        self.assertFalse(self.archive.exists())
        self.assertFalse(self.archive.with_name(self.archive.name + '.sha256').exists())

    def test_symlinks_and_existing_checksum_do_not_overwrite_or_leave_partial_archive(self):
        sidecar = self.archive.with_name(self.archive.name + '.sha256')
        sidecar.write_text('existing checksum')
        self.assertNotEqual(self.bundle().returncode, 0)
        self.assertEqual(sidecar.read_text(), 'existing checksum')
        self.assertFalse(self.archive.exists())
        sidecar.unlink()
        (self.release / 'LICENSE').unlink()
        (self.release / 'LICENSE').symlink_to(ROOT / 'LICENSE')
        self.assertNotEqual(self.bundle().returncode, 0)
        self.assertFalse(self.archive.exists())
