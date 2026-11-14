# Protected Gateway exec and authority prerequisite

`secretary-core service gateway-exec --manifest FILE --plan FILE` is an explicit
Linux operator command. It replaces its own process with the selected Gateway;
it creates no resident launcher, supervisor, worker or Core listener. It is not
an offline check. Invoke it only for an authorized target; development tests use
a disposable container and synthetic executable. Legacy startup/unit behavior is
unchanged. No automatic invocation or installation is introduced.

The command returns status 2 for usage or 1 for any denied/failed launch and writes
no diagnostic to inherited stdio. Success is exec, so the Gateway supplies its
eventual exit status. Standard I/O must satisfy the existing descriptor contract:
null or correctly directed anonymous pipes. Direct journald sockets, regular-file
stdio, terminals and other inherited handles are denied before configuration is
loaded. Operator pipes must not supply private Core data or interpret Gateway
output as privileged commands. The command requires all real/effective/saved/fs
UIDs and GIDs to be zero and exactly one thread at entry.

## Protected plan

The plan uses strict bounded JSON decoding (unknown/duplicate keys denied), with
schema `ciel.readonly.gateway-plan.v1` and exactly these fields:

| Field | Contract |
| --- | --- |
| `schema` | Exact version above |
| `deployment_generation` | Exact protected manifest generation |
| `manifest_sha256`, `policy_sha256` | Lowercase SHA-256 of the exact bytes consumed by the protected snapshot |
| `executable` | Absolute path through root-owned, non-writable, ACL-free ancestors; no symlinks |
| `executable_sha256` | Lowercase SHA-256 of the actual held executable inode |
| `arguments` | Explicit argument array, excluding argv[0]; at most 64 entries |
| `environment` | Explicit object, with no inherited environment |
| `working_directory` | Explicit absolute directory, entered after credential drop |
| `intent_file` | New absolute filename in a root:Core-group directory with mode 0750 |

The plan itself must be a root:Core-group regular file, mode 0440 or 0640, with one
link, no ACL and safe rooted ancestors, using the same loader as service checks.
Paths use the manifest's restricted absolute-path grammar. A single argument or
environment value is at most 8192 bytes, and their combined content plus environment
keys is at most 16 KiB; NUL is forbidden. Only `HOME`, `PATH`, `LANG`, `LC_ALL`,
`OPENCLAW_CONFIG_PATH` and `OPENCLAW_STATE_DIR` are accepted environment keys. Loader
injection, Node options, proxy and Core credential environment entries are not
accepted. Do not put credentials in plan arguments or values. Gateway-owned auth/
configuration must be independently supplied through its authorized account.

The selected executable must be a regular root-owned ELF with one link, no ACL,
no writable group/other permissions or set-ID bits, executable by all classes,
and the exact selected digest. Scripts are unsupported: retaining an interpreter
fd would violate the post-exec descriptor contract. Invoke a protected ELF runtime
with explicit script arguments when appropriate. This pins the ELF inode, not its
dynamic libraries, Gateway JS tree, plugin activation or live compatibility;
those remain release/host qualification responsibilities.

## Exec ordering and protected intent

Before reading protected configuration, the launcher requires the kernel's
`fs.suid_dumpable` to be 0 or 2, then sets NoNewPrivileges and disables dumpability.
Mode 1 is unsupported because a credential change would make the transient
process traceable to an existing Gateway peer before exec wipes its old memory.
No host sysctl is changed. Root administrators and their concurrent changes remain
trusted. NoNewPrivileges must be set while still root, closing the opportunity for
a privilege-regaining exec during credential drop; it cannot be deferred until
after changing UID. Dumpability is explicitly disabled again after UID/GID changes.

The launcher loads the protected non-secret snapshot and plan, verifies the actual
ELF, and observes its own boot ID, PID/start ticks, namespace identities and UID/GID
maps. It publishes one intent with exclusive `openat(O_CREAT|O_EXCL|O_NOFOLLOW)` in
the held protected directory. It writes/syncs under root-only mode, assigns the
Core group, publishes mode 0440, syncs the inode and directory. Existing names,
including partial/old intents and symlinks, are never overwritten or deleted.
Failure may leave an intent; an operator must inspect it and select a fresh plan/
intent path for another attempt. No automatic restart/replay authorization exists.

The intent schema is `ciel.readonly.gateway-intent.v1`. It contains only the schema,
deployment generation, manifest/policy/plan/executable digests, PID, start ticks,
boot ID, and `context` with namespace `device`/`inode` pairs plus numeric UID/GID
map rows. It contains no arguments, environment values, file bodies or credentials.

Before lowering credentials, all snapshot, plan, intent and ancestor handles are
closed. The only retained input is the exact read-only CLOEXEC ELF descriptor.
The launcher clears supplementary groups, all capability bounding bits, ambient
capabilities and keep-caps/securebits, sets real/effective/saved GIDs and UIDs to
the protected Gateway identity, clears permitted/effective/inheritable capability
sets and verifies the already-established NoNewPrivileges. Unknown capability
widths and failed syscalls deny.
The same process verifies all four IDs, all five empty capability sets, no extra
groups, one thread and only permitted descriptors. It enters the working directory
as Gateway and uses `execveat(AT_EMPTY_PATH)` on the held ELF. CLOEXEC removes the
last protected descriptor on successful exec. No process forks in this command.

## Core verification and limits

`ReadOnlySnapshot::check_gateway_process(plan_path)` first checks the actual Core
process using its retained protected policy. It loads the protected plan and intent,
checks their bindings and metadata, opens a CLOEXEC pidfd, and checks current boot/
start ticks and liveness. It requires the launch namespace identities for `cgroup`,
`ipc`, `mnt`, `net`, `pid`, `pid_for_children`, `time`, `time_for_children`, `user`
and `uts` to equal the Core's own current namespaces. UID/GID mappings must be
well-formed, nonempty, nonoverlapping and identical for launch, Core and Gateway.
All required namespace support must exist; there is no fallback for a separate
mount/user/PID sandbox. A broader namespace contract requires separate work.

Core observes every current Gateway thread's status (bounded to 4096 threads),
requiring each thread's UID/GID maps to match the launch context, all four IDs to
match Gateway, no unrelated supplementary groups,
NoNewPrivileges and five zero capability sets. Thread membership must be stable
across each scan; process observations repeat, the pidfd must remain live and all
protected inputs are revalidated. Threads created after guarded exec are permitted;
Core's own pre-runtime guard remains single-threaded. New mapping/capability or
process observations must never be inferred from local account files.

Cross-UID `/proc/PID/fd`, namespace links and executable links are deliberately
not assumed readable. Inherited-fd provenance comes from this root-controlled
launcher before exec, bound to a root-published intent, and current unprivileged
kernel identity. Root administrators are trusted. The intent is **pre-exec**:
neither it nor a successful authority check proves exec success, the Gateway host,
model, plugin, Google access, release compatibility or an application-ready state.
This is a point-in-time authority prerequisite, not a permanent admission token.
Runtime composition must recheck before consuming secrets and retain a suitable
liveness/authority policy; the complete runtime/admission lifecycle is unfinished.
The library-only [pre-runtime secret snapshot](runtime-secrets.md) now retains
these exact plan/executable/intent inputs and repeats authority checks before
secret access and borrowed-input access. Lifetime enforcement remains pending.

Offline `service check` remains incomplete with the existing four blockers. This
command/method does not open runtime secret bodies, enable writes, change approval/
scheduler/native-memory qualification, start Core or qualify a target/live host.
The operator credential, cloud credentials and Core journal remain inaccessible to
the Gateway identity through the existing DAC policy. Budget and runtime-secret
contracts and full release/admission gates remain required before new Core serving.

## Local Linux evidence workflow

`scripts/acceptance-service-check-linux.sh` runs the actual locked Rust build and
protected synthetic acceptance suite, including real gateway-exec payloads. A
separate `python3 -B scripts/gateway_launch_no_nnp_linux.py /target/debug/secretary-core`
run uses the same disposable root container setup/capabilities and compiled target
without Docker's `no-new-privileges` option. It first requires the parent kernel
status to show NNP=0, then executes both single-threaded and threaded payload cases.
The independent Core checker explicitly sets its own NNP; the root Gateway command
receives no such wrapper. This proves the launcher establishes NNP itself. Neither
test invokes an actual OpenClaw host, credentials, provider or live target.

Kernel API references: [execveat](https://man7.org/linux/man-pages/man2/execveat.2.html),
[capability bounding drops](https://man7.org/linux/man-pages/man2/PR_CAPBSET_DROP.2const.html),
[pidfd_open](https://man7.org/linux/man-pages/man2/pidfd_open.2.html), and
[user namespaces](https://man7.org/linux/man-pages/man7/user_namespaces.7.html).
The ordering/memory contract also follows
[NoNewPrivileges](https://man7.org/linux/man-pages/man2/PR_SET_NO_NEW_PRIVS.2const.html)
and [dumpability](https://man7.org/linux/man-pages/man2/PR_SET_DUMPABLE.2const.html).
