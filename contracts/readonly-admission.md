# Read-only release compatibility and admission

The manifest runtime checks compatibility and verifies an administrator-published
promotion before creating the production listener, Google client or operational
journal. It uses the existing protected manifest, Gateway plan and Core identity.
The operator selects the installed plugin directory explicitly. The legacy
installer and format-2 distribution layout continue to apply.

## Release compatibility

Run `service check-release-readonly --manifest FILE --plugin-dir DIRECTORY`
with the selected installed Core executable under its protected service identity.
Exit 0 means compatibility passed. Exit 1 denotes denial and exit 2 denotes
invalid arguments. The command checks the effective bootstrap authority, uses
the protected loader and never opens secret bodies.

Compatibility requires native Linux GNU x86-64 or AArch64, matching architecture
and glibc version, and a supported recorded build toolchain. The lock hashes and
five shipped configuration/unit templates must match the running Core's reviewed
source. Its process executable must be the same inode as the selected release's
hashed executable. The release contains exactly the format-2 files and their
required directories. The plugin archive must use the pinned SDK's gzip/USTAR
layout and exact package metadata, tool manifest and entry path. The selected
installed plugin directory must contain the same three files byte for byte.
The compatibility checks attest installed artifact consistency. The operator
selects the reviewed release and separately qualifies effective OpenClaw routing
and activation.

## Bounded operator qualification

Use the reviewed `scripts/readonly-qualification.py qualify` supervisor on the
selected Linux target only after explicit authorization for the bounded live
reads. Supply `--core`, `--manifest`, `--gateway-plan`, `--plugin-dir`,
`--uid`, `--gid`, `--plan`, and `--output`. Output must be a new directory
under a protected administrator-owned parent. The service manifest remains the
source of truth for the selected UID and GID. The supervisor runs as root and
starts one Core child with an empty environment, zero capability bounding set,
cleared supplementary groups and NoNewPrivileges. Core verifies its actual
root parent and binds child lifetime to that parent. The parent independently
enforces its deadline and waits for the child to exit.

Use the [example plan](../config/readonly-qualification-plan.example.json)
as a shape reference. Publish the privately selected plan as a protected
root-owned file readable by Core. The first three cases check missing/wrong
bearer and mutation denial. The remainder must cover each readable Sheets view
and one chosen tab per readable Docs registration exactly once. The plan permits
up to 20 cases and up to 180 seconds, with 2 MiB per capture and 8 MiB total.
A case reads at most one row or one Unicode scalar. Core asserts the returned
source binding and the exact continuation cursor when present.

The child reserves the production loopback port and invokes the real Core HTTP
router in process. This makes it impossible for other local requests to reach
the qualification router. A busy port denies qualification. The child uses the
same protected snapshot, lifetime guard and read-only provider constructor as
admitted startup. Qualification uses a private output directory, creates no
journal, and publishes no listener that serves HTTP. The operating system
terminates the child at its own deadline even if the supervisor is unavailable.

The child delivers only a digest over the supervisor's private pipe. The
supervisor verifies the capture bytes against that digest, checks the child
PID, exit status and port release, and publishes a root-owned private evidence
directory. Its final `supervisor.json` records what the parent observed.
The evidence directory contains private response captures and must remain
administrator-only. Interrupted or failed runs cannot be used for review.
Only the production qualification path emits `operator_live_core` evidence.

## Review, promotion and startup

Independently review the approved plan, private captures, provenance and
observed cleanup. After review, invoke `scripts/readonly-qualification.py review`
with the same Core/manifest/Gateway/plugin/UID/GID flags and
`--evidence DIRECTORY --reviewer LABEL --valid-for-hours HOURS --output NEW_DIRECTORY`.
The reviewer label is audit metadata. Authority comes from administrator
publication of protected files. Review verifies the original captures and
supervisor observation, then invokes Core's status-only `service review-readonly`
to revalidate the current effective setup. A successful review publishes
`promotion.json` exclusively. Review must occur within 24 hours of the run,
and the operator selects a lifetime of 1 to 720 hours. Review can carry no
simulation, partial coverage, failed cleanup or expired evidence into admission.

The canonical effective binding is SHA-256 of compact UTF-8 JSON using sorted
object keys, original array order and exact scalar types. Its domain is
`ciel.readonly.effective-binding.v1`. It includes host machine identity, generation,
the original registry, exact owner and credential principal, derived read scopes,
budget and launcher policies, Gateway plan, immutable release and installed plugin
path, protected-file authority/metadata, state-directory identity and fixed
endpoints. The credential principal comes from the same in-memory snapshot as
the key. Token/key contents and hashes are excluded. Raw file digests separately
bind the exact approved plan, run, and supervisor observation.

After separate permanent-start authorization, use
`service start-readonly --manifest FILE --gateway-plan FILE --plugin-dir DIRECTORY --promotion FILE`.
Use the installed executable under the protected Core identity with a clean
environment. Startup checks the current effective binding and all protected
evidence before reserving the loopback listener. The Core retains held inputs
for lifetime checks. Removing promotion authority blocks subsequent launches,
and administrators use explicit service stop/restart to revoke active access.

Evidence covers the selected Core reads, the auth denials, and the protected
lifecycle. The record lists plugin authority, host footprint, native memory,
approval, scheduler, delivery and provider writes as unchecked. Local fixtures
exercise compatibility and denial paths. Target qualification and service
activation are separate operator actions.
