# Ciel

Ciel is a lightweight personal secretary for OpenClaw: an owner-only TypeScript
plugin, one loopback Rust Core, one in-process command bus and a durable SQLite
operation FSM. Registered Google Sheets and Docs are general capabilities; the
wedding workbook is an optional domain profile. OpenClaw owns reminders and memory.

Project instructions start at [AGENTS.md](AGENTS.md). Versioned rules cover
[code/comments](.agent/rules/code.md), [FSM](.agent/rules/fsm.md),
[memory](.agent/rules/memory.md) and [processing](.agent/rules/processing.md).
Design notes, progress, glossary and task records stay local and are not required
to build or stage a release.

## Capabilities and boundaries

Use registered bounded Sheets views and Docs tabs through the
[API contract](contracts/openapi.yaml) and [generic registry example](config/workspace.example.json).
Controlled edits and tab creation require immutable previews, exact owner approval,
source conflict checks and the shared journal. New tabs receive no automatic
registration or access grant. Ambiguous writes reconcile through observation,
without another provider send.

Production writes, approval/scheduler qualification and native summary persistence
remain disabled by default. Local tests and simulated Google/model responses do
not qualify live Google, host authority, model output, channels or recovery. Keep
credentials, private targets, workbook contents and raw logs out of Git.

## Verification

Use actual Cargo/Bun locks and the pinned OpenClaw SDK. The qualified toolchain is
Python >=3.11, Rust/Cargo 1.95.0, Bun 1.2.21 and OpenClaw 2026.10.7. The SDK Node
engine range is `>=24.22.1 <25 || >=26.12.1`.

With the required dependencies available, run `bash scripts/verify.sh` for local
Rust/operator/plugin/SDK checks. Provider changes also need relevant scenarios in
`bash scripts/acceptance-all.sh` with Postman CLI 1.75.0. These exercise real
Core/bridge/journal against simulated providers. Use disposable targets and private
logs, and report local/simulated/live evidence separately.

## Installation

The installer stages a commit-bound native release, verifies its hashes and installs
inactive files. These are separate steps; none starts services, registers/activates
plugins, writes live Google data or changes production qualification flags.

Select a reviewed full 40-character commit in a clean checkout. The operator supplies
SECRETARY_REVISION and a private, operator-owned secretary_release_parent directory
outside that checkout. Build/stage as an unprivileged operator on the target native
platform, with the toolchain above and actual dependency locks:

```sh
python3 scripts/install.py plan --revision "$SECRETARY_REVISION"
python3 scripts/install.py stage --revision "$SECRETARY_REVISION" \
  --output "$secretary_release_parent/release"
python3 scripts/install.py verify --revision "$SECRETARY_REVISION" \
  --release "$secretary_release_parent/release"
```

Optional stage --offline requires previously populated frozen JS dependencies and
cached Cargo packages. Verify the independently selected commit and native OS/CPU/libc;
receipt hashes detect drift, not authenticity of an untrusted receipt. Never transfer
incompatible ARM/x86 binaries. Only after target-specific installation authorization:

```sh
sudo python3 scripts/install.py install-files --revision "$SECRETARY_REVISION" \
  --release "$secretary_release_parent/release"
```

Real filesystem installation requires Linux/root; --root supports a private disposable
root exercise. Existing config/state is preserved; conflicting releases, units and
unsafe paths fail closed. The release contains Core, the 26-tool three-file SDK plugin
archive, MIT license, preflight/backup utilities, inactive templates, this README as
INSTALLATION.md and a commit/hash receipt. The standalone installer shipped in a release
supports verify/install-files without a source checkout.

File installation does not create service users, provision private config, reload
systemd, start Core or register the plugin. Active setup and qualification require
separate owner authorization. Before activation, qualify the actual runtime,
Google ACL/API/scopes and disposable writes, owner-only full-preview approval,
model/native memory policy, scheduler/channel delivery and backup/recovery.
Keep production flags off until those gates and explicit activation authorization
are satisfied. Plugin --no-enable alone does not override an existing enabled
host policy; inspect effective policy before separately authorized registration.

Licensed under the [MIT License](LICENSE).
