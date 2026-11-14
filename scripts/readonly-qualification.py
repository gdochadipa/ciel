#!/usr/bin/env python3
"""Supervise an explicitly authorized read-only qualification and publish reviewed evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import socket
import stat
import subprocess
import sys
import tempfile
import time

MAX_BUNDLE = 50 * 1024 * 1024
MAX_RECORD = 65536
HEX = re.compile(r'[0-9a-f]{64}')
PATH = re.compile(r'(?:/[A-Za-z0-9_.-]+)+')


class Denied(Exception):
    pass


def require(condition):
    if not condition:
        raise Denied()


def exact_json(data):
    def object_pairs(pairs):
        result = dict(pairs)
        require(len(result) == len(pairs))
        return result

    def invalid_number(_):
        raise Denied()

    return json.loads(data, object_pairs_hook=object_pairs, parse_constant=invalid_number)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encode(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':')) + '\n').encode()


def protected_path(path, *, directory=False, private=False):
    require(PATH.fullmatch(str(path)) and not {'..', '.'}.intersection(path.parts))
    for ancestor in reversed(path.parents):
        info = ancestor.lstat()
        require(stat.S_ISDIR(info.st_mode) and info.st_uid == 0 and not info.st_mode & 0o7022)
    info = path.lstat()
    kind = stat.S_ISDIR if directory else stat.S_ISREG
    require(kind(info.st_mode) and info.st_uid == 0 and not info.st_mode & 0o7022)
    require(directory or info.st_nlink == 1)
    require(not private or not info.st_mode & 0o077)


def read_file(path, limit, uid, mode=None):
    descriptor = os.open(path, os.O_PATH | os.O_CLOEXEC | os.O_NOFOLLOW)
    try:
        info = os.fstat(descriptor)
        require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_uid == uid)
        require(mode is None or stat.S_IMODE(info.st_mode) == mode)
        require(info.st_size <= limit)
        with open(f'/proc/self/fd/{descriptor}', 'rb') as stream:
            data = stream.read(limit + 1)
            after = os.fstat(stream.fileno())
        require((after.st_size, after.st_mtime_ns, after.st_ctime_ns)
                == (info.st_size, info.st_mtime_ns, info.st_ctime_ns))
        require(len(data) <= limit)
        return data
    finally:
        os.close(descriptor)


def write_new(path, data, gid, mode):
    with path.open('xb') as stream:
        fd = stream.fileno()
        os.fchown(fd, 0, gid)
        os.fchmod(fd, mode)
        stream.write(data)
        stream.flush()
        os.fsync(fd)


def command(args, action, extra):
    return [
        '/usr/bin/setpriv', '--bounding-set=-all', '--no-new-privs',
        f'--reuid={args.uid}', f'--regid={args.gid}', '--clear-groups',
        str(args.core), 'service', action,
        '--manifest', str(args.manifest), '--gateway-plan', str(args.gateway_plan),
        '--plugin-dir', str(args.plugin_dir), *extra,
    ]


def supervise(argv, seconds):
    with subprocess.Popen(argv, env={}, cwd='/', stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                          start_new_session=True) as child:
        try:
            stdout, _ = child.communicate(timeout=seconds)
        except (subprocess.TimeoutExpired, KeyboardInterrupt, InterruptedError):
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait()
            raise Denied() from None
        require(child.returncode == 0)
        return child.pid, stdout


def port_released():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(('127.0.0.1', 8787))
        listener.listen(1)


def verify_capture(bundle_bytes):
    bundle = exact_json(bundle_bytes)
    require(type(bundle) is dict and set(bundle) == {'run', 'captures'})
    run, captures = bundle['run'], bundle['captures']
    require(type(run) is dict and run.get('schema') == 'ciel.readonly.run.v1')
    require(run.get('evidence_class') == 'operator_live_core')
    observations = run.get('observations')
    require(type(observations) is list and type(captures) is list)
    require(4 <= len(observations) == len(captures) <= 20)
    total = 0
    for observation, capture in zip(observations, captures, strict=True):
        require(type(observation) is dict and type(capture) is dict and set(capture) == {'response'})
        require(type(capture['response']) is str)
        data = capture['response'].encode()
        require(0 < len(data) <= 2 * 1024 * 1024)
        require(len(data) == observation.get('response_bytes') and sha(data) == observation.get('response_sha256'))
        total += len(data)
    require(total <= 8 * 1024 * 1024)
    return run


def qualify(args):
    protected_path(args.plan)
    plan_bytes = read_file(args.plan, MAX_RECORD, 0)
    plan = exact_json(plan_bytes)
    require(type(plan) is dict and type(plan.get('wall_seconds')) is int)
    require(1 <= plan['wall_seconds'] <= 180)
    protected_path(args.output.parent, directory=True)
    args.output.mkdir(mode=0o700)

    try:
        return qualify_into(args, plan, plan_bytes)
    except (Denied, OSError, ValueError, KeyError, TypeError, RecursionError):
        write_new(args.output / 'failure.json', encode({'qualification_recorded': False}), 0, 0o600)
        raise


def qualify_into(args, plan, plan_bytes):
    port_released()
    with tempfile.TemporaryDirectory(prefix='ciel-qualify-', dir='/var/tmp') as scratch:
        os.chown(scratch, args.uid, args.gid)
        capture = Path(scratch) / 'qualification.json'
        child, receipt = supervise(
            command(args, 'qualify-readonly', [
                '--plan', str(args.plan), '--output', str(capture),
                '--supervisor-pid', str(os.getpid()),
            ]),
            plan['wall_seconds'] + 15,
        )
        bundle = read_file(capture, MAX_BUNDLE, args.uid, 0o600)
        require(receipt == (sha(bundle) + '\n').encode())
        run = verify_capture(bundle)
        require(run.get('instance_pid') == child and run.get('plan_sha256') == sha(plan_bytes))
        port_released()
    run_bytes = encode(run)
    observation = encode({
        'schema': 'ciel.readonly.supervisor.v1',
        'run_sha256': sha(run_bytes),
        'plan_sha256': sha(plan_bytes),
        'bundle_sha256': sha(bundle),
        'instance_pid': child,
        'exit_status': 0,
        'port_released': True,
    })
    write_new(args.output / 'qualification.json', bundle, 0, 0o600)
    write_new(args.output / 'plan.json', plan_bytes, 0, 0o600)
    write_new(args.output / 'run.json', run_bytes, 0, 0o600)
    write_new(args.output / 'supervisor.json', observation, 0, 0o600)
    return {'qualification_recorded': True}


def review(args):
    protected_path(args.evidence, directory=True, private=True)
    records = {
        name: read_file(args.evidence / name, limit, 0, 0o600)
        for name, limit in [
            ('qualification.json', MAX_BUNDLE), ('plan.json', MAX_RECORD),
            ('run.json', MAX_RECORD), ('supervisor.json', MAX_RECORD),
        ]
    }
    run = verify_capture(records['qualification.json'])
    require(encode(run) == records['run.json'])
    supervisor = exact_json(records['supervisor.json'])
    require(supervisor == {
        'schema': 'ciel.readonly.supervisor.v1',
        'run_sha256': sha(records['run.json']),
        'plan_sha256': sha(records['plan.json']),
        'bundle_sha256': sha(records['qualification.json']),
        'instance_pid': run['instance_pid'],
        'exit_status': 0,
        'port_released': True,
    })
    require(1 <= args.valid_for_hours <= 720)
    require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{0,127}', args.reviewer))
    require(HEX.fullmatch(run['binding']))
    protected_path(args.output.parent, directory=True)
    args.output.mkdir(mode=0o750)
    os.chown(args.output, 0, args.gid)
    os.chmod(args.output, 0o750)
    for name in ('plan.json', 'run.json', 'supervisor.json'):
        write_new(args.output / name, records[name], args.gid, 0o440)
    now = int(time.time())
    promotion = {
        'schema': 'ciel.readonly.promotion.v1',
        'deployment_generation': run['deployment_generation'],
        'binding': run['binding'],
        'run_sha256': supervisor['run_sha256'],
        'run_file': str(args.output / 'run.json'),
        'plan_file': str(args.output / 'plan.json'),
        'supervisor_file': str(args.output / 'supervisor.json'),
        'supervisor_sha256': sha(records['supervisor.json']),
        'reviewer': args.reviewer,
        'reviewed_at': now,
        'expires_at': now + args.valid_for_hours * 3600,
    }
    candidate = args.output / '.promotion.pending'
    write_new(candidate, encode(promotion), args.gid, 0o440)
    try:
        _, stdout = supervise(
            command(args, 'review-readonly', ['--promotion', str(candidate)]), 30,
        )
        require(stdout == b'')
        os.link(candidate, args.output / 'promotion.json')
    finally:
        candidate.unlink()
    fd = os.open(args.output, os.O_DIRECTORY | os.O_CLOEXEC)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)
    return {'promotion_published': True}


def interrupt(_signal, _frame):
    raise InterruptedError()


def main():
    signal.signal(signal.SIGTERM, interrupt)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('qualify', 'review'))
    for name in ('core', 'manifest', 'gateway-plan', 'plugin-dir', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    for name in ('uid', 'gid'):
        parser.add_argument('--' + name, type=int, required=True)
    parser.add_argument('--plan', type=Path)
    parser.add_argument('--evidence', type=Path)
    parser.add_argument('--reviewer')
    parser.add_argument('--valid-for-hours', type=int)
    args = parser.parse_args()
    os.umask(0o077)
    try:
        require(os.geteuid() == 0 and 0 < args.uid < 2**32 - 1 and 0 < args.gid < 2**32 - 1)
        protected_path(args.core)
        protected_path(args.manifest)
        protected_path(args.gateway_plan)
        protected_path(args.plugin_dir, directory=True)
        require(PATH.fullmatch(str(args.output)) and args.output.name not in {'.', '..'})
        if args.action == 'qualify':
            require(args.plan is not None and args.evidence is None
                    and args.reviewer is None and args.valid_for_hours is None)
            result = qualify(args)
        else:
            require(args.plan is None and args.evidence is not None
                    and args.reviewer is not None and args.valid_for_hours is not None)
            result = review(args)
    except (Denied, OSError, ValueError, KeyError, TypeError, RecursionError):
        print(json.dumps({'ok': False, 'error': 'readonly_qualification_denied'}))
        return 1
    print(json.dumps({'ok': True, **result}))
    return 0


if __name__ == '__main__':
    sys.exit(main())
