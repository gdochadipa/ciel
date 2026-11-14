# Read-only runtime authority boundary

The manifest route retains the private protected snapshot; borrowed RuntimeInputs
are data, never admission. Bootstrap still checks the actual single-threaded Core,
all inherited handles, current Gateway authority and protected configuration before
secret access. A consuming transition checks bootstrap again before permitting
runtime-created threads/resources. It creates no client, listener or journal and
does not satisfy release/evidence/promotion admission by itself.

Runtime checks revalidate held metadata for the prepared owner, tokens, key and
budget policy. Gateway checks continue to validate the protected plan and intent. For held
and freshly rooted directories, require the same inode/device, owner/group, mode
and directory type, plus the existing rooted no-symlink/no-ACL policy. Directory
contents/size/link count/timestamps may change: the Core journal creates its own
lock, database, WAL and SHM. Regular protected input stamps remain exact. Neither
changed authority nor replacement input is silently adopted. This directory rule
is selected only by the consuming transition; offline/bootstrap checks retain
their original full-stamp semantics.

At each command boundary, check all actual Core threads against the protected
Core identity, zero capabilities, NoNewPrivileges and the captured namespace/maps.
Check Gateway with the pinned plan/ELF/intent and live boot/start/thread evidence.
Inspect Core descriptors without reading bodies. The private snapshot handles
must retain their original read/O_PATH direction and CLOEXEC. Stdio retains the
bootstrap null/one-way-pipe contract. Runtime-owned CLOEXEC TCP/UDP sockets and Linux
epoll/eventfd handles are permitted after bootstrap. The pinned Tokio signal
driver's anonymous stream socketpair is permitted only when SO_PEERCRED equals
the actual Core PID/UID/GID and both socket addresses are unnamed. External/named
Unix sockets, root peers, arbitrary extra
pipes, devices and unrelated regular files remain denied. Journal descriptors
must match one of the four rooted Core-owned 0600 single-link regular inodes
`secretary.lock`, `secretary.sqlite3`, `secretary.sqlite3-wal`,
`secretary.sqlite3-shm`, without ACLs. Callers cannot supply an fd/path allowlist.
UDP permits the resolver's unprivileged DNS transport, never raw/Unix sockets.
Network classification does not attest TLS or the remote application: the production
factory must separately pin its Google endpoints and read capabilities.

Before dispatch and before delivering its result, the read-only bus awaits a
serialized authority check. A failed check permanently fences that in-memory
runtime; later restoration cannot revive it. A fresh protected startup/admission is
required. A failure after an in-flight read withholds that result. It does not
undo/cancel an already completed provider read or claim continuous kernel
surveillance. No provider write is allowed. Separate operator recovery retains
authentication and is never charged against the model's task budget.

These checks observe current authority at command boundaries; they are not a
remote kill switch or a response to an authenticated promotion-revocation command.
No new monitor process, helper daemon, background queue, arbitrary input reload,
privileged descriptor adoption or production activation is introduced. The startup
route verifies release compatibility and protected promotion before it creates a
client, listener or journal. Bounded qualification uses the same guard and its own
operator authorization. See [read-only compatibility and admission](readonly-admission.md).
