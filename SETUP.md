# Set up Ciel on an existing OpenClaw host

This guide targets OpenClaw **2026.10.7** on an Ubuntu VPS. Keep your existing
OpenClaw installation, model provider and WhatsApp login. Ciel adds a plugin and
one local Rust Core; it does not require another OpenClaw instance.

```text
WhatsApp owner -> existing OpenClaw -> secretary-tools plugin -> local Core -> Google
```

The product and optional skill are called **Ciel**. The current plugin ID is
**`secretary-tools`**, and its private npm package name is
`@personal-secretary/openclaw-tools`. Use that plugin ID in OpenClaw commands.

## What installation does

| Stage | Result | Still needed |
| --- | --- | --- |
| Download + verify | A commit-bound package with matching hashes | Target installation |
| `install-files` | Core files and an inactive systemd unit | Account, configuration, credentials and plugin registration |
| Plugin install with `--no-enable` | Registered plugin files | Effective owner policy, shared tokens, running Core and activation |
| Ciel skill install | Instructions discoverable by the agent | Available tools and their existing permissions |
| Offline checks | Valid configuration shapes and permissions | Real host/Google qualification |
| Qualified activation | Running Core and enabled plugin | Verify actual owner-only tool calls |

The current installer does **not** automate all these stages. This guide makes
the remaining setup explicit. Use Bash on the VPS for the commands below, as the
same unprivileged user that runs OpenClaw; use `sudo` only where shown.

## 1. Check the host and keep a backup

```bash
uname -m
getconf GNU_LIBC_VERSION
python3 --version
node --version
openclaw --version
openclaw config validate --json
openclaw config file
```

The downloadable native package targets **Linux x86_64 / glibc 2.39** (Ubuntu
24.04). Python must be at least 3.11. The pinned OpenClaw SDK requires Node
`>=24.22.1 <25 || >=26.12.1`. Rust and Bun are build prerequisites when building
from source; they are not needed just to place prebuilt files.

Back up OpenClaw before changing plugin configuration:

```bash
(umask 077; openclaw backup create --no-include-workspace --verify)
```

Keep the archive private: it can contain credentials and native state. This
OpenClaw backup does not cover the separate `/etc/secretary` and
`/var/lib/secretary` directories. If Ciel is already installed, read
[Existing installations](#existing-installations) before file installation.

## 2. Download, verify and install the Core files

Choose a reviewed full 40-character commit from the repository's GitHub release.
Set it locally; the placeholder below must be replaced:

```bash
export CIEL_REVISION=REPLACE_WITH_FULL_REVIEWED_COMMIT
```

For a first installation, or after an explicitly reviewed retirement of an old
inactive installation:

```bash
(
  set -euo pipefail
  umask 077
  [[ "$CIEL_REVISION" =~ ^[a-f0-9]{40}$ ]] || {
    echo 'Set CIEL_REVISION to the reviewed full commit first.' >&2
    exit 1
  }
  ciel_work=$(mktemp -d "$HOME/ciel-install.XXXXXXXX")
  cd "$ciel_work"
  ciel_archive="ciel-$CIEL_REVISION-linux-x86_64.tar.gz"
  ciel_url="https://github.com/gdochadipa/ciel/releases/download/build-$CIEL_REVISION"
  curl -fL "$ciel_url/$ciel_archive" -o "$ciel_archive"
  curl -fL "$ciel_url/$ciel_archive.sha256" -o "$ciel_archive.sha256"
  sha256sum -c "$ciel_archive.sha256"
  tar -xzf "$ciel_archive"
  cd "ciel-$CIEL_REVISION"
  python3 install.py verify --revision "$CIEL_REVISION" --release "$PWD/release"
  sudo python3 install.py install-files \
    --revision "$CIEL_REVISION" --release "$PWD/release"
  sudo systemctl daemon-reload
)
```

Expected results: checksum `OK`, `"verified": true`, and
`"files_installed": true`. The receipt must retain `starts_services: false`,
`activates_plugins: false` and `production_writes: false`. The installer checks
the native OS/CPU/libc and refuses conflicting installations. A checksum detects
drift; use the trusted repository and independently selected revision as well.

Keep the extracted package and its standalone `install.py` for later verification.
For source builds, use the [maintainer installation commands](README.md#installation).

## 3. Install the plugin without activating it

First inspect existing effective policy for this plugin. `--no-enable` preserves
policy; it does not disable an already enabled plugin. For the inactive setup in
this guide, keep `secretary-tools` disabled before installing a replacement.

Run as the existing OpenClaw user, **without `sudo`**:

```bash
openclaw plugins install /opt/secretary/plugins/secretary-tools.tgz --no-enable
openclaw plugins inspect secretary-tools --json
```

Review the local-source and capability consent presented by OpenClaw. Do not use
`--force` to work around an unexplained error. A previously uninstalled plugin
keeps an explicit disabled marker until separately enabled.

Expected: the installed archive and manifest are present, `enabled: false`,
`activated: false`, and a disabled status. Current source declares **33 tools**;
declared/accepted contracts are not evidence that those tools are running.

## 4. Prepare the Core account and Google credential

Reuse your existing service-account JSON if it is the intended Google identity.
Core runs as `secretary`, and its unit uses `ProtectHome=true`, so a credential
stored in an OpenClaw user's home needs a protected copy outside `/home`.

Set the actual existing path locally, without pasting its contents into chat:

```bash
export CIEL_GOOGLE_KEY=/absolute/path/to/your/service-account.json
```

If `/etc/secretary/google-service-account.json` is already correctly provisioned,
reuse it and skip the copy block. Otherwise:

```bash
sudo bash -s -- "$CIEL_GOOGLE_KEY" <<'BASH'
set -euo pipefail
umask 077
test -f "$1"
test ! -L "$1"
test ! -e /etc/secretary/google-service-account.json
test ! -L /etc/secretary/google-service-account.json
if ! getent group secretary >/dev/null; then
  groupadd --system secretary
fi
if ! getent passwd secretary >/dev/null; then
  useradd --system --gid secretary --home-dir /var/lib/secretary \
    --no-create-home --shell /usr/sbin/nologin secretary
fi
test "$(id -u secretary)" -ne 0
install -d -o root -g secretary -m 0750 /etc/secretary
install -d -o secretary -g secretary -m 0700 /var/lib/secretary
install -o secretary -g secretary -m 0600 \
  "$1" /etc/secretary/google-service-account.json
runuser -u secretary -- test -r /etc/secretary/google-service-account.json
BASH
```

Keep the original file. Inspect metadata without exposing its key:

```bash
sudo stat -c '%n | owner=%U | mode=%a | type=%F' \
  /etc/secretary/google-service-account.json
```

Expected: owner `secretary`, mode `600`, regular file. To find the account email
for Google's Share dialog, print only that field locally:

```bash
sudo -u secretary python3 - <<'PY'
import json
from pathlib import Path
data = json.loads(Path('/etc/secretary/google-service-account.json').read_text())
if data.get('type') != 'service_account':
    raise SystemExit('Expected a service-account credential.')
print(data['client_email'])
PY
```

Ensure the intended Google Cloud project has the Sheets and Docs APIs enabled.
Give this service account **Viewer** access to the selected Sheets and Docs for
the initial read checks. Reuse existing access where appropriate. Sharing and
file permissions alone do not prove OAuth/API access. No paid quota upgrade is
required by this setup guide.

## 5. Register the selected Google resources

The registry restricts Ciel to explicitly selected files and bounded Sheets
ranges; it does not discover every file in Google Drive. Create it only if absent:

```bash
sudo bash <<'BASH'
set -eu
test ! -e /etc/secretary/registry.json
test ! -L /etc/secretary/registry.json
install -o secretary -g secretary -m 0600 \
  /opt/secretary/templates/registry.example.json /etc/secretary/registry.json
BASH
sudoedit /etc/secretary/registry.json
```

Edit these fields in the generic example:

| Field | Value to enter locally |
| --- | --- |
| Spreadsheet `alias` | A short name the owner can use, such as `operations` |
| Spreadsheet `id` | ID between `/spreadsheets/d/` and the next `/` in its URL |
| View `name` | A short view name, such as `overview` |
| View `sheet_id` | Numeric `gid` of the exact tab; `0` is valid |
| View `title` | Exact tab title |
| View `range` | A bounded A1 rectangle, such as `A1:D20` |
| Document `alias` | A short name, such as `notes` |
| Document `id` | ID between `/document/d/` and the next `/` in its URL |

Leave `readable: true`, `create_tabs: false` and document `writable: false`.
Use `kind: "grid"` for general Sheets reads; keep its headers/formulas/merges
empty as in the example. Do not add `writable_range` or a write schema for this
initial setup. Remove unused resource entries instead of retaining example IDs.
When revising a registration later, use a new registry version and requalify the
affected effective setup. A newly created tab is not automatically registered.

Validate with the installed Core, without starting it:

```bash
sudo -u secretary /opt/secretary/bin/secretary-core registry check \
  --config /etc/secretary/registry.json --profile readonly
```

Expected: `ok: true`, `mode: "offline"`, the intended resource counts, and zero
configured write permissions. `live_access_checked: false` is intentional.
Keep the raw registry and target IDs private; share only the status/summary when
asking for help.

## 6. Bind the WhatsApp owner and Core/Gateway tokens

Use the WhatsApp owner number in E.164 form, such as the **synthetic example**
`+15555550123`, consistently in Core and plugin settings. Use the intended
incoming owner's identity; the Gateway's logged-in WhatsApp number is not
automatically that owner. Verify the actual native sender/owner mapping during
host qualification.

For files not yet present, copy the installed templates with exclusive creation:

```bash
sudo bash <<'BASH'
set -euo pipefail
for pair in service.env.example:service.env qualification.example.json:qualification.json; do
  source_name=${pair%%:*}
  target_name=${pair#*:}
  if [[ -e "/etc/secretary/$target_name" || -L "/etc/secretary/$target_name" ]]; then
    printf 'Preserved existing %s\n' "$target_name"
    continue
  fi
  install -o secretary -g secretary -m 0600 \
    "/opt/secretary/templates/$source_name" "/etc/secretary/$target_name"
done
BASH
sudoedit /etc/secretary/service.env
```

Replace every placeholder in `service.env`. Set `SECRETARY_OWNER` to the chosen
owner, `SECRETARY_CONFIG` to `/etc/secretary/registry.json`, and
`GOOGLE_APPLICATION_CREDENTIALS` to the protected credential copy. Keep the
default state directory and loopback connection.

Generate two independent random tokens **on the VPS**, using a local secret
manager or this command, and copy them locally into their respective fields:

```bash
python3 -c 'import secrets; print(secrets.token_hex(32)); print(secrets.token_hex(32))'
```

The first is `SECRETARY_CORE_TOKEN`; the second is
`SECRETARY_APPROVAL_TOKEN`. They must differ and each must have at least 32
characters. Treat the generated output as secret: keep it out of chat, screenshots,
Git and support logs. Retain `SECRETARY_WRITES_ENABLED=false`,
`SECRETARY_WRITE_TASKS_ENABLED=false` and `SECRETARY_MOCK_MODE=false`.

Provide the same two tokens to the **existing Gateway process** through its
protected environment mechanism. OpenClaw supports the global
`${OPENCLAW_STATE_DIR:-$HOME/.openclaw}/.env`; preserve unrelated entries and keep
the file private and owned by the OpenClaw user. Set only the two Ciel token
variables there, not a copy of Core's entire environment file. Existing process
environment takes precedence, so resolve stale overrides and arrange the approved
Gateway reload/restart before testing. Do not print the environment to diagnose it.

Set the plugin's owner and keep its optional capabilities off. Replace the
synthetic owner number below before running the command:

```bash
openclaw config set plugins.entries.secretary-tools.config '{
  "owner": "+15555550123",
  "coreUrl": "http://127.0.0.1:8787",
  "approvalTransportQualified": false,
  "taskToolsEnabled": false,
  "taskTransportQualified": false,
  "schedulerQualified": false,
  "summaryMemoryEnabled": false,
  "readOnlyRuntime": false
}' --strict-json
openclaw config validate --json
```

Apply this fragment to the Ciel entry; preserve the rest of your existing config.
The repository's full OpenClaw example is a reference for a dedicated deployment,
not a replacement for your current model, channel, memory and tool policies.
Review WhatsApp owner admission and effective tool allow/deny policy separately;
an inbound allowlist does not restrict outbound messages.

Here, read-only means the initial **resource permissions and writes-disabled
configuration**. The separate `readOnlyRuntime` profile needs the protected
manifest, read-task budget and admission procedure; enabling that boolean alone
does not set them up. See [read-only admission](contracts/readonly-admission.md).

## 7. Install the Ciel usage skill

The skill teaches the model how to select registered resources, cite partial
reads, use the enabled approval workflow and handle uncertain outcomes. It does
not install Core, enable tools, create credentials or grant access.

The skill is a separate source file, not part of the plugin's three-file runtime
archive. From a reviewed repository checkout that contains `skills/ciel/SKILL.md`,
run as the OpenClaw user:

```bash
export CIEL_SOURCE=/absolute/path/to/reviewed/ciel-checkout
openclaw skills install "$CIEL_SOURCE/skills/ciel"
openclaw skills info ciel --json
openclaw skills check
```

For a download-only host, fetch just that skill from a reviewed full source
commit **containing the skill**, then install the local directory:

```bash
export CIEL_SKILL_REVISION=REPLACE_WITH_REVIEWED_COMMIT_CONTAINING_THE_SKILL
(
  set -euo pipefail
  umask 077
  [[ "$CIEL_SKILL_REVISION" =~ ^[a-f0-9]{40}$ ]] || exit 1
  ciel_skill=$(mktemp -d "$HOME/ciel-skill.XXXXXXXX")
  mkdir "$ciel_skill/ciel"
  curl -fL \
    "https://raw.githubusercontent.com/gdochadipa/ciel/$CIEL_SKILL_REVISION/skills/ciel/SKILL.md" \
    -o "$ciel_skill/ciel/SKILL.md"
  openclaw skills install "$ciel_skill/ciel"
  openclaw skills info ciel --json
)
```

Older native releases can predate this skill file; a successful plugin install
does not mean that revision also contains the skill. Keep the reviewed skill
revision with your deployment notes. A local install does not publish to ClawHub.

OpenClaw discovers skills in the agent workspace and its managed skill roots.
If an agent has a skill allowlist, admit `ciel` through that existing policy.
Check visibility with `openclaw skills check --agent YOUR_AGENT_ID`. Start a new
session if the current skill snapshot has not refreshed. A workspace skill with
the same name can shadow a managed copy; inspect the path in `skills info`.

Try an owner request such as `$ciel list the registered Google files` after
activation. Natural-language discovery is also supported by the skill's
description. Skill discovery does not prove model selection or tool execution.

## 8. Qualify the host before activation

At this point, files and configuration are prepared. The shipped production
unit's `ExecStartPre` still requires actual host qualification. It checks an
operator-maintained, private `/etc/secretary/qualification.json`; the installed
example deliberately has `qualified: false`.

The current production gate requires evidence for runtime activation, owner and
outbound policy, disposable Sheets behavior, trusted complete-preview approval,
native scheduler restart/delivery, explicit memory policy, and backup/restore.
Readable Docs additionally require separate disposable Docs evidence. This gate
applies even with production writes disabled; file placement or Viewer sharing
does not complete it. Populate attestations only from real evidence. Keep all
optional capability flags off until their own checks and activation authorization.

For the separately protected read-only runtime, use its reviewed
[qualification, promotion and startup procedure](contracts/readonly-admission.md),
including the Gateway launcher and durable read-task budget. Do not substitute
that profile's promotion for the production unit's attestation or vice versa.

For the production unit, check the existing attestation and local prerequisites
as the service user:

```bash
sudo -u secretary python3 /opt/secretary/scripts/preflight.py \
  --env-file /etc/secretary/service.env \
  --qualification-file /etc/secretary/qualification.json
```

An unqualified setup is expected to fail here. Preflight itself does not call
Google or establish the evidence it checks. Keep Core stopped until qualification
is complete. After successful qualification and explicit activation authorization,
start the configured Core, enable the reviewed plugin and apply the Gateway's
required environment reload using its existing service manager:

```bash
sudo systemctl start secretary-core.service
openclaw plugins enable secretary-tools
```

Review any capability consent rather than bypassing it. Inspect both services
and the plugin's effective state. Enable boot persistence only after the initial
owner-only test succeeds. An existing tool allowlist may need the selected Ciel
read tools admitted without replacing unrelated policy.

From the authorized WhatsApp owner, list registered resources, describe the
chosen view/tab, and request a small read from each registered resource. Confirm
the actual source binding and coverage. Metadata-only list success does not prove
that Google content was read. Keep raw responses private. Expand permissions or
activate writes, task tools, reminders or memory only through their corresponding
qualification and owner decision.

## Existing installations

Keep a backup and verify the incoming package before removing working files.
`install-files` supports a verified same-revision replay, not automatic replacement
of a different revision. Keep the old package, Core unit/drop-ins, private config
and a coherent journal snapshot for rollback. Use Core's installed `backup.py`
for an existing journal; copying a live SQLite file alone is not a coherent backup.

For an explicitly selected inactive uninstall/reinstall, inspect and stop/disable
the named Core unit, preserve the old `/opt/secretary` and managed unit/drop-ins
under a private backup directory, then reload systemd before fresh installation.
Preserve `/etc/secretary` and `/var/lib/secretary`; first file installation may
never have created them, so skip absent directories when backing up. Investigate
conflicting vendor units or enablement rather than deleting them indiscriminately.

`openclaw plugins uninstall secretary-tools --dry-run` previews plugin removal.
Actual uninstall also removes plugin settings and matching allow/deny entries;
it does not remove Rust Core. Restore the intended owner configuration from your
private backup and verify effective policy before separately enabling a reinstall.
Keep unrelated OpenClaw state, workspaces and memory intact.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `/etc/secretary` or state directory missing | File installation does not provision configuration/state; complete step 4. |
| `disabled in config` | Expected during inactive setup; finish configuration and qualification before enabling. |
| `blocked by denylist` | Inspect the plugin's explicit effective policy; install flags do not override it. |
| `provenance-invalid` for an archive | The pinned SDK can classify a custom archive this way even with a matching install path. Read the separate activation reason; the label is not a payload-integrity check. Use supported lifecycle commands, not registry database edits. |
| Core cannot read a key in `/home` | Use the protected service-owned copy outside the unit's `ProtectHome` boundary. |
| Core unavailable or unauthorized | Check service status, loopback URL and matching token sources; omit token values from logs. |
| Google access denied | Check service-account identity, API enablement, sharing, selected IDs and effective read scopes. |
| Registry invalid | Check numeric `gid`, exact title, bounded range, unique aliases and disabled write permissions. |
| Skill present but no Ciel tools | Skill discovery and plugin activation are separate; inspect plugin and agent tool policy. |
| Preview/approval succeeds but outcome is uncertain | Inspect the same operation's history; request observation-only reconciliation, without another provider send. |
| Preflight denied | Missing/unsafe configuration or incomplete actual host evidence; keep the gate intact. |

When asking for help, provide the reviewed commit, command exit status, redacted
plugin status and offline summary. Keep tokens, private keys, target IDs, personal
contents and raw provider logs on the host.
