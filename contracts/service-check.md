# Offline read-only service check

`secretary-core service check --manifest FILE` inspects local configuration only.
It does not start Core, access Google, open/recover a journal, read secret bodies,
install files, modify permissions, enable a plugin or issue an admission receipt.
Unknown service commands/flags fail without falling back to legacy startup.

The executable supports the strict
[manifest schema](readonly-manifest.v1.schema.json) and
[service-policy schema](service-policy.v1.schema.json). Synthetic examples are
[manifest](../config/readonly-manifest.example.json) and
[policy](../config/service-policy.example.json); their identities/paths are
illustrative and are not deployment defaults or valid host authority.

## Service policy v1

All eight fields are mandatory; unknown fields, duplicate decoded JSON keys,
missing fields and wrong types are rejected.

| Field | Supported value |
| --- | --- |
| `schema` | `ciel.readonly.service-policy.v1` |
| `revision` | 1–128 ASCII bytes; first alphanumeric, remaining alphanumeric, `.`, `_`, `-`. |
| `core` | Exactly `{uid, gid}` as nonzero unsigned 32-bit integers. |
| `gateway` | Same shape; its UID and primary GID must both differ from Core. |
| `listener` | `127.0.0.1:8787` |
| `filesystem` | `linux-root-owned-no-acl.v1` |
| `launcher` | `ciel.readonly.launcher.v1` |
| `endpoints` | `google-production.v1` |

These identifiers select a static policy contract. They do not attest a running
unit's settings, effective supplementary groups or network behavior. The launcher
implementation/admission remains pending. The provisional 120-second processing
target does not change timeouts through this command.

## Inspection contract

The same bounded strict JSON decoder is used by the manifest/policy parser and
existing registry tooling. The manifest has twelve required fields, an exact
`readonly` profile and absolute ASCII paths of 2–4096 bytes using letters, digits,
`.`, `_`, `-`, `/`. Root-only paths, empty/dot components, trailing separators,
expansion and URI paths are rejected. All input paths/identities must be distinct;
configuration cannot live inside service-owned state. Registry permission checks
use the existing Rust domain validator, including dormant grants. At least one
readable Sheets or Docs registration is required.

The Linux adapter walks from `/` using held directory descriptors and `openat`
with `O_NOFOLLOW`. Ancestors must be root-owned, without special permission bits,
group/other writes or POSIX ACLs. It inspects access and default ACL xattrs through
held `/proc/self/fd` references; unavailable/unsupported ACL inspection fails closed.
Every inode is first opened with `O_PATH`; only verified regular non-secret files
are reopened for reading through that held inode, with metadata identity checked.
Special files are rejected before any readable open can invoke device behavior.
Sensitive files must be regular, singly linked, root-owned, Core-group-readable
`0440` or `0640`. The state directory must belong to Core UID/GID with mode `0700`.
Core directory traversal/file readability is checked against local account groups.
Public release files must be root-owned and non-writable by group/others.

Token, owner, Google credential and budget-policy references use
`O_PATH` descriptors for metadata only. Their contents are neither read nor hashed.
Local account resolution reads protected `/etc/passwd` and `/etc/group` directly;
it performs no NSS/LDAP lookup. Core and Gateway must match local numeric identities
and the Core group must exclude other non-root accounts. This local membership
check does **not** prove effective groups granted by systemd or another launcher;
that remains an explicit blocker. No Gateway isolation claim follows from it.

The separate [Core process-authority prerequisite](launcher-authority.md) provides
a library guard for kernel-observed credentials/capabilities. It is not called by
this offline command and does not remove its launcher/admission blocker. Future
startup must use it in the actual Core process before worker threads or secrets.

The checker validates the existing format-2 release inventory's shape, exact
eleven artifact names, disabled flags, digest shapes and actual bounded artifact
hashes. The separate protected release check inspects the installed plugin, the
shipped templates, the exact release tree, and the running executable. Offline
reports retain the compatibility blocker because they do not run those checks.
See [read-only compatibility and admission](readonly-admission.md).

Before returning a report, the adapter rechecks both held metadata and fresh
rooted path walks to reject concurrent identity/content/ancestor changes. These
checks provide one offline observation; they confer no future runtime authority
and do not defend against an administrator replacing the executable/policy.

The Linux filesystem contract is unsupported on macOS/other platforms and is
reported as denied. Parser and decision-logic tests can run there; they do not
qualify Linux `openat`/`O_PATH`/xattr behavior.

## Protected non-secret snapshot API

`infrastructure::service_check::ReadOnlySnapshot::load(&Path)` is now the shared
protected loader behind the CLI and the library. It performs the existing complete
static inspection once, retains the typed manifest/policy/registry in memory and
keeps file/ancestor descriptors and metadata observations alive until drop. It
cannot be constructed by deserializing a caller's registry/policy and exposes no
mutable fields, Clone, Debug or serialization implementation.

`registry()` and `deployment_generation()` borrow the retained validated values;
`report()` returns the existing sanitized incomplete report. Each rechecks all held
and current path metadata before returning. These operations do not reopen paths
to decode a different configuration. A copied registry is data, not a transferable
file-authority/admission capability. Access times are not part of snapshot binding.

`check_current_process()` rechecks the protected files, checks the kernel process
against the retained policy, then rechecks the files again. There is no policy
argument through which a caller can substitute another identity. Call it inside
the future actual Core before threads or runtime secrets. Changed files/ancestors
are rejected; the object never silently reloads or adopts a replacement generation.
Already borrowed data is not a permanent revocation mechanism, and administrative
configuration changes still require the future explicit reload/restart procedure.

This object is an immutable **non-secret static prerequisite**, not the complete
effective runtime configuration. Owner, token, key and budget bodies remain
metadata-only. The four blockers describe the scope of the offline report.
The library takes an explicit manifest path and reads no environment configuration; the CLI retains its existing environment-
conflict denial. The protected startup separately validates runtime secrets, binding and admission
before constructing clients or listeners.

## Results and exit status

| Exit | Meaning |
| --- | --- |
| `1` | Denied: invalid/unsafe configuration, conflicting environment, unavailable input or unsupported file-authority platform. |
| `2` | Invalid service command/arguments. |
| `3` | Static checks completed; readiness is incomplete and service admission is unavailable. |

There is no exit-0 ready/admitted result in this implementation. A valid static
snapshot explicitly reports these unresolved requirements:

- `budget_policy_runtime_validation_pending`: the policy body and durable task
  context require the protected runtime path; the offline check never reads them.
- `launcher_authority_pending`: effective launcher/group policy is not verified.
- `release_contract_and_compatibility_pending`: full package compatibility/admission is unverified.
- `runtime_secret_validation_pending`: owner/token/key bodies, token separation and credential principal remain unchecked.

Reports contain only bounded counts/statuses and sanitized codes, never source
IDs/aliases, identity values, paths, tokens, keys, secret hashes or parser errors.
They always set `configuration_changed`, `activation_changed`, `live_access_checked`,
`service_started` and `journal_opened` to false. A valid static report still has
`ok:false`, `readiness:"incomplete"`; automation must preserve the exit status.

Inherited `SECRETARY_*`, `GOOGLE_APPLICATION_CREDENTIALS` and HTTP/HTTPS/ALL/NO_PROXY
keys are rejected case-insensitively, even if empty; values are never logged.
The command has no environment, test-root, account, endpoint or `--force` override.
Legacy no-argument service startup and registry commands retain their own routes.

## Reproducible local Linux acceptance

Run `bash scripts/acceptance-service-check-linux.sh` with Docker and the locked
Cargo registry already cached locally. Build the local verification image first
with `docker build -f deploy/Dockerfile.verify -t secretary-verification:2026.10.7 deploy`
if it is not available; image construction may require network access. The
acceptance launcher never pulls an image, installs dependencies or accesses the
network. It mounts source and the dependency cache read-only and uses a disposable
build directory. No host configuration or account file is modified.

The container runs formatting, all-target Clippy with warnings denied, the full
locked offline Rust suite (including Linux O_PATH/ACL primitives), then real CLI
acceptance on protected synthetic files. The CLI cases cover root and non-root
Core static completion, sanitized denials, symlinks, hard links, FIFO/device
rejection, access/default ACLs, ancestor authority, private file/state permissions,
local identities, duplicate keys, dormant writes and release hash/executable checks.
Fixture snapshots verify unchanged content, identities, permissions, modification/
change times, ACLs and state (access times are excluded). Kernel inotify
watches, first checked against a control read, verify no readable open/access of
owner, token, credential or budget inodes by the non-root CLI.

Only fixture setup/inspection uses the container's ownership, permission and
device capabilities; the non-root CLI has no supplementary groups. The fixture
uses the image's existing Debian account identities without editing account files.
An unsupported filesystem/ACL/inotify environment fails the suite; it is not
silently skipped. This establishes local Linux adapter/CLI evidence only. It does
not establish target-platform release compatibility, effective launcher isolation,
runtime-secret validation, service admission or live Google/host qualification.
