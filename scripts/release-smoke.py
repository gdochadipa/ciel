#!/usr/bin/env python3
"""Native Core offline registry smoke under 2 GiB / two CPUs; no host activation."""
import argparse
import json
import os
from pathlib import Path
import resource
import subprocess
import tempfile
import install
import release

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--release', type=Path, required=True)
parser.add_argument('--revision', required=True)
args = parser.parse_args()
receipt = install.verify_release(args.release, args.revision)
if receipt['build_host'] != release.TARGET or install.native_host() != release.TARGET:
    raise SystemExit('smoke requires native Ubuntu x86_64/glibc 2.39')
binary = args.release / 'bin/secretary-core'
header = binary.read_bytes()[:20]
if header[:6] != b'\x7fELF\x02\x01' or header[18:20] != b'\x3e\x00':
    raise SystemExit('expected ELF64 little-endian x86_64 binary')


def limits():
    resource.setrlimit(resource.RLIMIT_AS, (2 * 1024**3, 2 * 1024**3))
    resource.setrlimit(resource.RLIMIT_CPU, (20, 20))
    os.sched_setaffinity(0, sorted(os.sched_getaffinity(0))[:2])


with tempfile.TemporaryDirectory(prefix='ciel-smoke-') as home:
    result = subprocess.run([str(binary.resolve()), 'registry', 'check', '--config',
                             str((args.release / 'templates/registry.example.json').resolve())],
                            env={'HOME': home, 'PATH': '/usr/bin:/bin'}, cwd=home,
                            capture_output=True, text=True, timeout=30, preexec_fn=limits)
    if result.returncode or json.loads(result.stdout).get('ok') is not True:
        raise SystemExit('bounded offline Core smoke failed')
print(json.dumps({'ok': True, 'scope': 'offline_registry_cli_only', 'cpu_limit': 2,
                  'address_space_limit_bytes': 2 * 1024**3,
                  'peak_child_rss_kib': resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,
                  'live_qualified': False, 'full_openclaw_capacity_qualified': False}))
