"""Emit bounded, allowlisted acceptance failure metadata; never echo raw logs."""
import argparse
import json
import os
from pathlib import Path
import re

STAGES = ('sdk_build', 'core_build', 'core_artifact', 'simulation_start',
          'provider_address', 'client', 'traffic')
CATEGORIES = {
    'address_in_use': r'\beaddrinuse\b|address already in use',
    'permission_denied': r'\beacces\b|permission denied|operation not permitted',
    'command_missing': r'command not found|no such file or directory',
    'connection_failed': r'\beconnrefused\b|\benotfound\b|could not resolve host',
    'unsupported_option': r'unknown option|unknown command|unrecognized argument',
    'authentication_required': r'authentication required|not logged in|please log in',
}


def tail(path):
    try:
        with path.open('rb') as stream:
            stream.seek(0, 2)
            stream.seek(max(0, stream.tell() - 65536))
            return stream.read(65536).decode('utf-8', errors='replace')
    except OSError:
        return ''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--log-dir', required=True, type=Path)
    parser.add_argument('--scenario', required=True,
                        choices=('read', 'writes', 'retry', 'drift', 'error', 'slow', 'workspace'))
    parser.add_argument('--stage', required=True, choices=STAGES)
    parser.add_argument('--exit-code', required=True, type=int)
    parser.add_argument('--simulation-exit-code', type=int)
    args = parser.parse_args()
    logs = {name: tail(args.log_dir / name) for name in
            ('sim.ndjson', 'sim.err', 'core.log', 'client.log', 'build.ndjson', 'build.err', 'sdk.log', 'parser.err')}
    events = {'listening': 0, 'request': 0, 'error': 0}
    for line in logs['sim.ndjson'].splitlines():
        try:
            event = json.loads(line)
        except (ValueError, RecursionError):
            continue
        if isinstance(event, dict) and isinstance(event.get('event'), str) and event['event'] in events:
            events[event['event']] += 1
    text = '\n'.join(logs.values()).lower()
    diagnostic = {
        'format_version': 1, 'scenario': args.scenario, 'stage': args.stage,
        'exit_code': args.exit_code, 'simulation_exit_code': args.simulation_exit_code,
        'event_counts_in_log_tails': events,
        'error_categories': sorted(name for name, pattern in CATEGORIES.items()
                                   if re.search(pattern, text)),
    }
    encoded = json.dumps(diagnostic, sort_keys=True)
    (args.log_dir / 'diagnostic.json').write_text(encoded + '\n')
    print('Acceptance failure metadata: ' + encoded)
    summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if summary:
        with open(summary, 'a', encoding='utf-8') as stream:
            stream.write('\n### Secretary acceptance failure\n\n```json\n' + encoded + '\n```\n')


if __name__ == '__main__':
    main()
