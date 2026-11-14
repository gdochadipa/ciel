# Read-only Core process authority prerequisite

The separate [protected Gateway exec contract](gateway-launch.md) defines the
explicit transient launcher and snapshot-bound Gateway authority check. Neither
path grants full runtime/service admission.

`ciel.readonly.launcher.v1` requires checking the actual Core process before any
runtime threads, secrets, journal or listener are created. Numeric local account
membership and systemd template text do not prove effective process authority.

The Rust library entry point
`infrastructure::service_launcher::check_current_process(&ServicePolicy)` validates
the selected policy and observes the calling process on Linux. Policy must come
from the administrator-protected immutable effective configuration; accepting a
caller-deserialized policy alone does not authenticate it. This guard observes
authority, never changes credentials/capabilities, spawns a process or starts Core.

The protected `service_check::ReadOnlySnapshot` now supplies that policy provenance
for the non-secret static phase. Its `check_current_process()` method checks held/
current file authority before and after calling this guard with the retained
policy. A caller cannot replace its policy through this method. The offline CLI
shares the snapshot loader but still does not call the process guard; complete
effective runtime configuration and startup/admission remain unimplemented.

## Required kernel observation

- All real, effective, saved and filesystem UIDs equal the selected non-root Core
  UID. All four GIDs equal its selected non-root primary GID.
- Supplementary groups contain no group except the selected Core primary group.
  An empty set and a repeated primary group are both accepted; an unrelated group
  is denied even if local account files do not list it.
- Inheritable, permitted, effective, bounding and ambient capabilities are all zero.
- `NoNewPrivs` is enabled, preventing exec from acquiring privilege through file
  capabilities or set-ID executables.
- The process has exactly one thread. The guard must run before a Tokio runtime
  or another worker is started because Linux credentials are per-thread.
- No unknown inherited descriptors remain. Standard I/O can use `/dev/null`, an
  anonymous read pipe for stdin or anonymous write pipes for stdout/stderr.
  Regular files, named FIFOs and connected sockets are rejected even on stdio;
  extra channels above descriptor 2 are rejected. A socket can retain a privileged
  connection established before credential drop. Named FIFO type alone is not
  anonymous-pipe evidence: the adapter checks pipefs through `fstatfs`.

The snapshot-bound method admits only its own held read-only/O_PATH file and
ancestor handles, all still close-on-exec. No public argument supplies an fd
allowlist. The primitive policy-only guard admits no protected-file exceptions.
Two sorted descriptor observations must agree, bounded to 65,536 entries.
Only the exact held procfs enumeration directory's handles are introspection
exceptions. Existing handles are inspected with fstat/fcntl/fstatfs, never reopened
or read. Unexpected handles are not closed or repaired silently.

Startup must fail without writing a diagnostic into unsafe inherited stdout/stderr;
an exit status alone is appropriate until stdio authority is established. Existing
legacy service units/stdout configuration are unchanged. In particular, systemd's
direct journal Unix socket needs a separately reviewed logging arrangement for
the new path; it is not treated as a safe pipe by this guard.

Read `/proc/thread-self/status` from a held descriptor and verify its filesystem
is procfs. Bound each read to 64 KiB, require every authority field exactly once
and reject malformed/overflowing numbers, missing fields and ambiguous shapes.
Other kernel status fields are ignored because they are unrelated and vary by
kernel version. Two independent authority observations must agree. The result
exposes only success or a sanitized code, never IDs, group lists or capability
masks. Missing/untrusted procfs and non-Linux platforms fail closed.

This is a point-in-time prerequisite, not a permanent seal. The calling Core must
keep its validated identity and avoid starting privileged helpers or changing
credentials after the check. A launcher must arrange these properties before
executing Core; the guard must not silently repair an unsafe launch.

## Admission remains unavailable

The guard is library-only. It is not wired into legacy startup, `service check`,
qualification or permanent startup; those retain their current behavior. Future
manifest-based startup must call it from the same single-threaded process that
will consume the protected effective snapshot, before opening runtime secrets.
Calling a separate checker process does not attest the eventual Core process.

Full Gateway/runtime composition, effective service-manager overrides,
release compatibility, budgets and runtime-secret validation require their own
contract/evidence. No check here authenticates another process or proves Gateway
isolation by this Core-only guard. Keep `launcher_authority_pending` and all other offline blockers until
the complete admission boundary is implemented and qualified. No service template,
qualification flag, deployed process or plugin is activated by this change.

## Local verification

The domain matrix covers every UID/GID slot and capability set, unexpected groups,
privilege gain and premature threading. Parser cases require every kernel field
and reject duplicate/missing/invalid shapes. A Linux test compares kernel status
with independent UID/GID/group/NoNewPrivileges syscalls. The existing local Docker
acceptance launcher compiles a test-only Rust probe against the actual locked Core
library and exercises clean non-root, primary-group repeat, extra group, retained
bounding capabilities, root/saved-root and multi-threaded launches. The probe is
not installed or exposed as a production CLI, and cannot supply a fake observation
to the library. These are local synthetic results, never host/service admission.
