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
tower={version="0.5",features=["util"]}
serde={version="1",features=["derive"]}
serde_json="1"
chrono="0.4"
sha2="0.10"
sqlx={version="0.7",default-features=false,features=["sqlite","runtime-tokio"]}
uuid={version="1",features=["v4"]}
tokio={version="1",features=["full"]}
''')
lines=[]
for name in ('domain','application','presentation'):
    lines.append('#[path='+json.dumps(str(src/'src'/name/'mod.rs'))+'] pub mod '+name+';')
lines.append('pub mod infrastructure { #[path='+json.dumps(str(src/'src/infrastructure/journal.rs'))+'] pub mod journal; }')
lines.append('pub use domain::{error::CoreError,registry::Registry,registry,projection,write_model}; pub use infrastructure::journal; pub use application::writes; pub use presentation::http::{router,AppState};')
(target/'src/lib.rs').write_text('\n'.join(lines))
for name in ('adapter','write_adapter','write_journal','command_bus'):
    (target/'tests'/f'{name}.rs').write_text((src/'tests'/f'{name}.rs').read_text())
common=(src/'tests/common/mod.rs').read_text().replace('"../../../../config/spreadsheets.example.json"',json.dumps(str(repo/'config/spreadsheets.example.json')))
(target/'tests/common/mod.rs').write_text(common)
PY
CARGO_TARGET_DIR="${TMPDIR:-/tmp}/secretary-adapter-target" cargo test --offline --manifest-path "$harness/Cargo.toml"
echo "Domain/application/presentation/journal evidence; production Reqwest/OAuth and socket tests require full cargo test. Harness: $harness"
