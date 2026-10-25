#!/usr/bin/env python3
"""Read-only production gate. Never sources env files or prints their values."""
import argparse
import json
import os
from pathlib import Path
import stat
import sys


def private_file(path):
    p = Path(path)
    info = p.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_mode & 0o077:
        raise ValueError('required file must be regular, service-owned and mode 0600 or stricter')
    return p


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--env-file', required=True)
    parser.add_argument('--qualification-file', default='/etc/secretary/qualification.json')
    args = parser.parse_args()
    try:
        if os.geteuid() == 0:
            raise ValueError('run preflight as the non-root service account')
        # Qualification always gates deployment, including when env configuration is incomplete.
        try:
            qualification = json.loads(private_file(args.qualification_file).read_text())
        except (OSError, ValueError):
            raise ValueError('host qualification record is unavailable or unsafe') from None
        required_evidence = ('runtime_activation', 'owner_and_outbound_policy', 'google_disposable_sheet',
                             'trusted_full_diff_approval', 'native_scheduler_restart_and_delivery',
                             'explicit_memory_policy', 'backup_restore')
        evidence = qualification.get('evidence', {})
        if qualification.get('qualified') is not True or qualification.get('openclaw_version') != '2026.10.7':
            raise ValueError('host qualification is incomplete or targets another OpenClaw version')
        if not isinstance(evidence, dict) or any(not isinstance(evidence.get(k), str) or not evidence[k].strip() for k in required_evidence):
            raise ValueError('host qualification evidence is incomplete')
        values = {}
        for line in private_file(args.env_file).read_text().splitlines():
            if not line.strip() or line.lstrip().startswith('#'):
                continue
            key, separator, value = line.partition('=')
            if not separator or not key.replace('_', '').isalnum() or key in values:
                raise ValueError('environment file contains unsupported or duplicate entries')
            values[key] = value
        for key in ('SECRETARY_CONFIG', 'SECRETARY_CORE_TOKEN', 'SECRETARY_APPROVAL_TOKEN', 'SECRETARY_OWNER', 'GOOGLE_APPLICATION_CREDENTIALS', 'SECRETARY_STATE_DIR'):
            if not values.get(key):
                raise ValueError('required service configuration is missing')
        tokens = (values['SECRETARY_CORE_TOKEN'], values['SECRETARY_APPROVAL_TOKEN'])
        if any(len(token) < 32 or 'REPLACE' in token for token in tokens) or tokens[0] == tokens[1]:
            raise ValueError('distinct non-placeholder bearer tokens of at least 32 characters are required')
        if values.get('SECRETARY_MOCK_MODE', '').lower() == 'true':
            raise ValueError('production startup rejects mock mode')
        state = Path(values['SECRETARY_STATE_DIR']).lstat()
        if not stat.S_ISDIR(state.st_mode) or state.st_uid != os.geteuid() or state.st_mode & 0o077:
            raise ValueError('state directory must be private and service-owned')
        if values.get('SECRETARY_WRITES_ENABLED', 'false').lower() != 'false':
            raise ValueError('production write activation is pending separate qualification')
        registry = json.loads(private_file(values['SECRETARY_CONFIG']).read_text())
        credentials = json.loads(private_file(values['GOOGLE_APPLICATION_CREDENTIALS']).read_text())
        if not isinstance(registry, dict) or credentials.get('type') != 'service_account':
            raise ValueError('registry or Google service-account file has an invalid shape')
        print('Preflight passed: operator qualification and local file checks only.')
        return 0
    except (OSError, ValueError, TypeError, AttributeError):
        # Deliberately omit exception text: parsers and paths can contain private values.
        print('Preflight denied: host qualification, private files or service configuration failed; consult deployment prerequisites.', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
