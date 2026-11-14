# Protected pre-runtime secret snapshot

`infrastructure::service_runtime::ReadOnlyRuntimeSnapshot::prepare(snapshot, plan)`
is an explicit library-only preparation step. It consumes the protected static
snapshot in the actual single-threaded Core. There is no new CLI/start command,
HTTP client, OAuth request, listener, journal or provider action. Legacy startup
and offline `service check` retain their existing behavior and four blockers.
This is a configuration prerequisite, not complete effective runtime/admission.

## Ordering and file binding

Before opening a runtime secret body, preparation rejects competing environment
names and checks the actual Core and Gateway using the existing protected launch
contract. It retains the exact protected Gateway plan, ELF and intent handles in
the snapshot. Those handles join only the snapshot's private descriptor allowlist;
callers cannot supply an allowlist. Existing inherited handles still fail closed.
The retained metadata prevents later checks from silently adopting a replacement
plan/intent/process under the same manifest generation. The pre-exec intent still
does not prove application readiness or a loaded OpenClaw plugin.

Owner, Core token, operator token and Google credentials are read once by promoting
their held O_PATH inodes to read-only CLOEXEC handles. No newly resolved filename
selects the secret. Revalidate held and rooted metadata before and after each read;
reject changes rather than reload. Reads are bounded before allocation and during
streaming. The protected budget policy is also read once (at most 64 KiB), decoded
strictly using [read-task budget v1](read-task-budget.md), and retained in the same
snapshot. Offline inspection still leaves that body unopened. State remains
metadata-only until an admitted composition opens the bound operational journal.
After promotion, later rooted revalidation uses O_PATH metadata handles rather
than reacquiring readable secret handles. It still compares the held inode and
every current rooted ancestor; a changed input is rejected without reloading.

Preparation ends by checking environment, pinned files and both actual processes
again. `check_pre_runtime()` and `inputs()` repeat these checks, including live
boot/start/thread/namespace/Gateway observations. `inputs()` lends the original
registry, owner, separate tokens, principal, validated signing key, generation and
state path. It creates no admission authority and performs no additional secret
read. Neither snapshot type nor the borrowed input wrapper implements Debug,
Clone, Serialize or Deserialize. Explicitly copied values remain ordinary data.

This API must run before runtime threads. Revalidation is point-in-time; no
lifetime service enforcement or revalidation interval is implemented. A future
runtime must define that policy and full admission before consuming these inputs
to serve. No secrets, values, private paths or their hashes enter reports/errors.
Memory zeroization, protection against a malicious root administrator and physical
secret erasure are not provided by this API.

## Text and token contract

Owner and token files contain one UTF-8 value with at most one final LF. Remove
only that LF; preserve other bytes exactly. Reject empty/whitespace-only values,
control characters, embedded CR/LF/NUL, invalid UTF-8 and additional records. Owner
content is at most 256 UTF-8 bytes; its trusted host-routing identity comparison
remains an admission responsibility.

Tokens contain 32–4096 ASCII bytes using the HTTP bearer alphabet
`A-Z a-z 0-9 - . _ ~ + /` with optional trailing `=` padding. Spaces and internal
padding are rejected. Core and operator token values must differ after LF framing.
Reject a body made of one repeated character and case-insensitive occurrences of
`replace`, `change-me`, `changeme`, `example`, `placeholder`, `your-token` or
`insert-token`. These are finite placeholder checks, not proof of entropy.
Provision independently generated random tokens; do not supply operator material
to Gateway. No token in this profile authorizes approval or apply.

## Google credential contract

Strict JSON, at most 64 KiB, rejects duplicate and unknown keys. Required fields:
`type=service_account`, `client_email`, `private_key`, and
`token_uri=https://oauth2.googleapis.com/token`. The supported principal is a
nonempty lower-case ASCII service-account name (at most 64 bytes) at a nonempty
project prefix followed by `.iam.gserviceaccount.com`, with total email length at
most 254 bytes. Local/project prefix characters are lower-case letters, digits,
hyphen, underscore or period. Other service-account email forms require an
explicit compatibility revision.

Optional standard metadata fields are `project_id`, `private_key_id`, `client_id`,
`auth_uri`, `auth_provider_x509_cert_url`, `client_x509_cert_url`, and
`universe_domain`. Optional strings may also be absent/null. A supplied auth URI
must equal `https://accounts.google.com/o/oauth2/auth`; a supplied universe must
equal `googleapis.com`. Metadata never selects an endpoint or principal.

Decode the RSA PEM with the locked JWT implementation and require a successful
local RS256 signature over a constant non-OAuth payload. Discard that signature.
Malformed/non-signing private-key material is rejected before any HTTP client or
OAuth. This proves local key usability only; it does not prove Google ownership,
registration, source ACLs, trusted host routing or live access. Principal and key
are retained from the same decoded credential snapshot. Never emit raw parser or
signing errors. The public `tests/support/synthetic-rsa.pem` is solely a disposable
test signing fixture, with no account, registration or provider access.

## Verification and remaining gates

Native unit tests cover text framing/bounds, tokens, separation, strict credential
shape, duplicate keys, pinned production settings and local RSA usability. New
protected Linux acceptance cases exercise process/environment denial before secret
access, inherited handles, once-read preparation, unread budgets, pinned input
changes and Gateway death. Execution evidence must be recorded separately in
local progress; merely adding those cases is not a Linux pass.

Trusted cumulative budgets, full release compatibility, qualification/promotion,
host-owner binding and a lifetime policy remain required before admitted startup.
Production/write/approval/scheduler/native-memory defaults remain disabled. No
deployment or live action is authorized by preparation or its local tests.
