#!/bin/sh
# Compiles actual adapter source with cached dependencies, never substitutes production HTTP/auth.
set -eu
repo=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
harness=$(mktemp -d "${TMPDIR:-/tmp}/secretary-adapter.XXXXXX")
python3 - "$repo" "$harness" <<'PY'
import pathlib,json,sys
repo=pathlib.Path(sys.argv[1]);target=pathlib.Path(sys.argv[2]);src=repo/'services/secretary-core'
(target/'src').mkdir();(target/'tests/common').mkdir(parents=True)
(target/'Cargo.toml').write_text('''[package]
name="secretary-core-domain"
version="0.1.0"
edition="2021"
[lib]
name="secretary_core"
[dependencies]
axum="0.8"
serde={version="1",features=["derive"]}
serde_json="1"
chrono="0.4"
sha2="0.10"
''')
lines=[]
for name in ('error','registry','projection'):
    lines.append('#[path='+json.dumps(str(src/'src'/f'{name}.rs'))+'] pub mod '+name+';')
lines.append('pub use error::ApiError; pub use registry::Registry;')
(target/'src/lib.rs').write_text('\n'.join(lines))
(target/'tests/adapter.rs').write_text((src/'tests/adapter.rs').read_text())
common=(src/'tests/common/mod.rs').read_text().replace('"../../../../config/spreadsheets.example.json"',json.dumps(str(repo/'config/spreadsheets.example.json')))
(target/'tests/common/mod.rs').write_text(common)
PY
cargo test --offline --manifest-path "$harness/Cargo.toml"
echo "Adapter-only evidence; production Reqwest/OAuth and public HTTP tests require full cargo test. Harness: $harness"
