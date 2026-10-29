# Offline Workspace registry setup

Use the same secretary-core binary to prepare/check a local registry without credentials, environment files, Google access, a journal or a listener. These are one-shot operator commands, separate from no-argument service startup and production preflight. The plugin still has 26 tools; it does not receive a configuration-editing tool.

These commands require a reviewed Core build from T030 or later. Verify the checkout/release receipt before execution: historical T027/T029 native artifacts do not establish CLI support. Older Core builds ignore command arguments and must not be probed with these offline commands. From a reviewed current checkout, `cargo run --offline --locked -p secretary-core -- registry ...` selects the current source build.

A reviewed 42f18d5 Linux ARM64 release has now passed staging/hash verification, disposable installation and installed registry init/check exercises without valid credentials or runtime state. This confirms CLI support only for that tested native artifact; verify the independently selected release receipt and target CPU/libc before use. It is not a VPS installation or live qualification.

## Create an inactive draft

Choose a new private directory outside the checkout. For a disposable draft on macOS/Linux:

```sh
secretary_registry_dir="$(mktemp -d)"
secretary_registry_file="$secretary_registry_dir/registry.json"
secretary-core registry init --output "$secretary_registry_file"
secretary-core registry check --config "$secretary_registry_file"
```

Init uses the shipped generic Workspace example, with every readable/create_tabs/Docs-writable/grid-write permission disabled. IDs, tab ID/title and ranges are synthetic placeholders; they do not discover or describe your Google resources. The file is created exclusively, with Unix 0600 permissions or stricter under the caller's umask. Existing files and final symlinks are refused; parent directories must already exist. It never replaces an active configuration. A write/disk error can leave a partial new draft: inspect it and choose a fresh output rather than treating it as usable.

Edit the draft locally before selecting it for later separately authorized host setup. Use unique aliases/IDs, verified tab IDs/titles and bounded read ranges. Keep mutation grants disabled until the corresponding qualification and explicit activation exist. Increment registry version when changing mappings/permissions. This CLI does not install the draft, reload Core, change environment flags or activate anything.

## Check a draft

Check reuses the exact Rust Registry deserialization/validator used by Core. It accepts a regular file up to 1 MiB, refuses final symlinks, and uses no-follow/nonblocking Unix reads before checking the opened file type. It neither reads credential files selected by environment variables nor opens Core state. Only macOS/Linux-style Unix offline setup is supported; other platforms fail closed for these commands.

The JSON response reports offline validity and aggregate resource/readable/configured-write counts. Those are declared registry grants, not effective Google ACLs, OAuth scope evidence, host permissions or runtime activation. live_access_checked and activation_changed remain false. No IDs, aliases, titles, paths, version values or raw parser errors are printed. Input bytes/mode stay unchanged. Production preflight and T007 qualification remain separate.

| Result | Meaning |
| --- | --- |
| Exit 0, ok:true | Local schema/domain validation passed, or a new inactive draft was created. |
| Exit 1, registry_json_invalid | Invalid/incomplete JSON; inspect the private file locally. |
| Exit 1, registry_schema_invalid | Wrong field/type/unknown key; use the documented registry structure. |
| Exit 1, registry_invalid_document/spreadsheet/view/grid_write_range/grid_range or registry_invalid | Existing Core registry rules rejected registration; inspect uniqueness, bounds and permission relationships. |
| Exit 1, registry_unavailable/unsafe_file/too_large | Input is unavailable, unsafe or exceeds the offline byte cap. |
| Exit 1, output_exists/unavailable/write_failed | Init refused an existing target or could not finish a new draft. |
| Exit 2, usage | Invalid/incomplete arguments; no service is started. |

JSON error codes are diagnostic categories; they deliberately omit private values. No-argument secretary-core retains the existing environment-configured service behavior. Do not mistake offline check success for authorization to start it or edit live resources. See [deployment](deployment.md), [installation](installation.md) and [T007](../.scratch/secretary-mvp/T007.md).
