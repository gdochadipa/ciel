# Offline Workspace registry setup

Use the same secretary-core binary to prepare/check a local registry without credentials, environment files, Google access, a journal or a listener. These are one-shot operator commands, separate from no-argument service startup and production preflight. The plugin still has 26 tools; it does not receive a configuration-editing tool.

Init/check require a reviewed Core build from T030 or later; new-resource add requires T031 or later; grid_view additions require T032 or later. Verify the checkout/release receipt before execution: historical T027/T029 native artifacts do not establish CLI support. Older Core builds ignore command arguments and must not be probed with these offline commands. From a reviewed current checkout, `cargo run --offline --locked -p secretary-core -- registry ...` selects the current source build.

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

## Add a resource to a new candidate

T031 accepts a small addition file instead of requiring the full registry schema. Keep the base and addition JSON in a private directory outside Git; IDs stay out of command arguments. Prepare the addition using your local editor, with one of these shapes (all values below are synthetic):

```json
{
  "version": "workspace-v2",
  "resource": {
    "kind": "document",
    "alias": "project-notes",
    "id": "example-document-id"
  }
}
```

For a spreadsheet, provide one to ten bounded grid views. No header/formula/merge/domain schema boilerplate is needed:

```json
{
  "version": "workspace-v3",
  "resource": {
    "kind": "spreadsheet",
    "alias": "inventory",
    "id": "example-spreadsheet-id",
    "views": [
      {"name": "stock", "sheet_id": 0, "title": "Stock", "range": "A1:D20"},
      {"name": "totals", "sheet_id": 7, "title": "Summary", "range": "B2:C8"}
    ]
  }
}
```

Use actual provider IDs, not share URLs; sheet_id is the numeric tab ID, not its position. Confirm IDs/titles/ranges separately: this offline command cannot discover resources or prove provider identity/access. Grid ranges must be bounded rectangles (single cell: B2:B2), <=10000 cells, <=100 columns and <=10000 rows, as enforced by Core.

Using the private directory from the init example, save the chosen JSON as addition.json, then run these options in the shown order:

```sh
secretary-core registry add \
  --config "$secretary_registry_file" \
  --resource "$secretary_registry_dir/addition.json" \
  --output "$secretary_registry_dir/candidate.json"
secretary-core registry check --config "$secretary_registry_dir/candidate.json"
```

Version must be nonblank and differ from the base. For document/spreadsheet additions, the candidate preserves existing resources/permissions semantically and appends one new resource; it does not reformat or modify the source files. New Docs are readable=false/writable=false; new Sheets are readable=false/create_tabs=false, with generic grid views and no write permissions. Permission fields in the addition are rejected rather than silently accepted. Existing init placeholders remain in a base created by init; edit/remove those synthetic placeholders deliberately before selecting a draft for real use. An empty base `{"version":"workspace-v1","spreadsheets":[],"documents":[]}` is also valid.

Both input files use the same bounded safe reads as check. The entire candidate is validated and capped at 1 MiB before an exclusive 0600-or-stricter output is created. Existing paths, including either input and final symlinks, cannot be overwritten. Repeating the same output returns output_exists; adding the same resource to a later candidate is rejected by version/uniqueness rules. To add another resource, use the previous candidate as --config, a fresh version and a fresh --output path. A disk/write failure can leave a partial new output, as with init.

Aliases and IDs must be unique across Sheets and Docs; view names must be unique within each spreadsheet. There is no merge/replace/suffix behavior. T032 also supports adding a grid view to an existing spreadsheet as described below. No Google file/tab is created. Review the candidate locally before separately authorized installation or permission changes. Readable Docs still requires its own T007 evidence; Sheets evidence does not qualify Docs. The older 42f18d5 release receipt covers init/check only, not add.

## Add a tab/range view to an existing draft spreadsheet

With a reviewed T032-or-later Core, use the same registry add command and save this shape in the private addition.json file (synthetic values):

```json
{
  "version": "workspace-v4",
  "resource": {
    "kind": "grid_view",
    "spreadsheet": "inventory",
    "view": {
      "name": "incoming-stock",
      "sheet_id": 8,
      "title": "Incoming",
      "range": "A1:D20"
    }
  }
}
```

Select the exact existing spreadsheet alias, not its Google ID, URL or a Docs alias. The parent may be readable or inactive. One new generic grid view is appended; its parent's ID/readable/create_tabs flags, previous views, Docs and existing write grants remain unchanged. No writable_range/write_schema or permission fields are accepted. The view inherits the parent's readable flag in the candidate: adding it to a readable parent explicitly registers the bounded range for reads if that candidate is later installed. Adding it to an inactive parent keeps it inaccessible. There is no separate per-view readable switch. Existing write permissions on overlapping views remain as declared; this operation adds none.

A new tab identity is allowed. Another named range on an already mapped tab is also allowed when its numeric sheet_id and exact title agree. The CLI rejects same ID/different title and same ASCII-case-insensitive title/different ID, without guessing a rename. Duplicate view names and more than ten views are rejected by Core. No old view is replaced or silently renamed. These local checks do not prove that the tab exists or matches live metadata; verify it separately. This command creates no Google tab, file or cells.

Use a changed version, the draft containing that spreadsheet as --config, and a fresh --output path. The same safe input, candidate validation/size/privacy and no-overwrite rules apply. Replaying into the same output fails; reapplying the same view name to a later candidate fails instead of duplicating it. The active configuration is never edited/reloaded. Older T030/T031 artifacts do not qualify grid_view support, and no T032 native installed-release receipt is claimed.

## Check a draft

Check reuses the exact Rust Registry deserialization/validator used by Core. It accepts a regular file up to 1 MiB, refuses final symlinks, and uses no-follow/nonblocking Unix reads before checking the opened file type. It neither reads credential files selected by environment variables nor opens Core state. Only macOS/Linux-style Unix offline setup is supported; other platforms fail closed for these commands.

The JSON response reports offline validity and aggregate resource/readable/configured-write counts. Those are declared registry grants, not effective Google ACLs, OAuth scope evidence, host permissions or runtime activation. live_access_checked and activation_changed remain false. No IDs, aliases, titles, paths, version values or raw parser errors are printed. Input bytes/mode stay unchanged. Production preflight and T007 qualification remain separate.

| Result | Meaning |
| --- | --- |
| Exit 0, ok:true | Local validation passed, or a new draft/candidate was created (existing grants may be retained). |
| Exit 1, registry_json_invalid | Invalid/incomplete JSON; inspect the private file locally. |
| Exit 1, registry_schema_invalid | Wrong field/type/unknown or duplicate key; use the documented input structure. |
| Exit 1, registry_spreadsheet_not_found | grid_view must select an existing spreadsheet alias. |
| Exit 1, registry_view_identity_conflict | New view contradicts an existing tab ID/title mapping; inspect the private draft locally. |
| Exit 1, registry_version_unchanged | Choose a different candidate version. |
| Exit 1, registry_bounds | Empty version or too many resources; add also rejects whitespace-only versions. |
| Exit 1, registry_invalid_document/spreadsheet/view/grid_write_range/grid_range or registry_invalid | Existing Core registry rules rejected registration; inspect uniqueness, bounds and permission relationships. |
| Exit 1, registry_unavailable/unsafe_file/too_large | Input is unavailable, unsafe or exceeds the offline byte cap. |
| Exit 1, output_exists/unavailable/write_failed | Init/add refused an existing target or could not finish a new draft. |
| Exit 2, usage | Invalid/incomplete arguments; no service is started. |

JSON error codes are diagnostic categories; they deliberately omit private values. No-argument secretary-core retains the existing environment-configured service behavior. Do not mistake offline check success for authorization to start it or edit live resources. See [deployment](deployment.md), [installation](installation.md) and [T007](../.scratch/secretary-mvp/T007.md).
