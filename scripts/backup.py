#!/usr/bin/env python3
"""Create a coherent SQLite journal snapshot using SQLite's backup API."""
import argparse
import os
from pathlib import Path
import sqlite3
import stat
import tempfile
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', help='existing journal database')
    parser.add_argument('destination', help='new backup file in a protected directory')
    args = parser.parse_args()
    temporary = None
    try:
        source = Path(args.source).resolve(strict=True)
        destination = Path(args.destination)
        source_info = source.stat()
        parent_info = destination.parent.stat()
        if not stat.S_ISREG(source_info.st_mode) or not stat.S_ISDIR(parent_info.st_mode):
            raise ValueError('invalid backup paths')
        if parent_info.st_uid != os.geteuid() or parent_info.st_mode & 0o077:
            raise ValueError('backup directory must be private and owned by this user')
        fd, temporary_name = tempfile.mkstemp(prefix='.secretary-backup-', dir=destination.parent)
        temporary = Path(temporary_name)
        os.close(fd)
        with sqlite3.connect(source.as_uri() + '?mode=ro', uri=True, timeout=10) as live:
            with sqlite3.connect(temporary, timeout=10) as backup:
                live.backup(backup)
                if backup.execute('PRAGMA integrity_check').fetchone() != ('ok',):
                    raise ValueError('backup integrity check failed')
        # Publish only the completed snapshot; link refuses any existing target.
        with temporary.open('rb') as snapshot:
            os.fsync(snapshot.fileno())
        os.link(temporary, destination)
        temporary.unlink()
        temporary = None
        print('SQLite backup created and integrity checked.')
        return 0
    except (OSError, ValueError, sqlite3.Error):
        if temporary is not None:
            temporary.unlink(missing_ok=True)
        print('Backup failed; source was not modified. Check private paths and SQLite availability.', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
