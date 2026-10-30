#!/usr/bin/env python3
"""Wrap a verified native release with its standalone, reviewed installer."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import sys
import tarfile
import install

MAX_BYTES = 100 * 1024 * 1024
TARGET = {'system': 'Linux', 'machine': 'x86_64', 'libc': ['glibc', '2.39']}


def bundle(release, revision, output):
    receipt = install.verify_release(release, revision)
    if receipt.get('build_host') != TARGET:
        raise install.InstallerError('download target must be Linux x86_64 with glibc 2.39')
    output = install.plain_path(output)
    checksum = output.with_name(output.name + '.sha256')
    if output.is_relative_to(release.resolve()):
        raise install.InstallerError('archive must be outside the release')
    files = {'install.py': Path(__file__).with_name('install.py')}
    files.update({'release/' + name: release / name for name in install.ARTIFACTS | {'release.json'}})
    if sum(path.stat().st_size for path in files.values()) > MAX_BYTES:
        raise install.InstallerError('release exceeds distribution size limit')
    # Exclusive reservation of both outputs; never overwrite an earlier distribution.
    created = []
    try:
        for path in (output, checksum):
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            os.close(fd)
            created.append(path)
        with output.open('wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', filename='', mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w|') as archive:
                for name, path in sorted(files.items()):
                    data = path.read_bytes()
                    member = tarfile.TarInfo('ciel-' + revision + '/' + name)
                    member.size = len(data)
                    member.mode = 0o755 if name in ('install.py', 'release/bin/secretary-core') else 0o644
                    archive.addfile(member, io.BytesIO(data))
        # Revalidate source after packaging so ordinary concurrent drift cannot be published.
        install.verify_release(release, revision)
        digest = install.fingerprint(output)
        checksum.write_text(digest + '  ' + output.name + '\n')
        return {'source_commit': revision, 'sha256': digest, 'bytes': output.stat().st_size,
                'production_writes': False, 'starts_services': False, 'activates_plugins': False}
    except BaseException:
        for path in created:
            path.unlink(missing_ok=True)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--release', type=Path, required=True)
    parser.add_argument('--revision', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        print(json.dumps(bundle(args.release, args.revision, args.output)))
        return 0
    except (install.InstallerError, OSError, ValueError, tarfile.TarError):
        print('release bundle failed: invalid target, input or exclusive output', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
