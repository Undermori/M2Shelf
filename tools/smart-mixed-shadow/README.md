# SMART_MIXED Phase 2 Shadow

Read-only Rust index adapter and standalone differential harness. Phase 3 reuses the adapter through the explicit `production` feature; default Shadow behavior remains schema 24. The adapter never writes SQLite or media. Production logical persistence lives in the desktop application's `smart_mixed.rs`, not in this harness.

From the repository root, with Rust/MSVC and the cached dependencies available:

```powershell
cargo test --offline --locked --manifest-path tools/smart-mixed-lab/Cargo.toml
cargo fmt --manifest-path tools/smart-mixed-shadow/Cargo.toml -- --check
cargo test --offline --locked --all-features --manifest-path tools/smart-mixed-shadow/Cargo.toml
cargo clippy --offline --locked --all-targets --all-features --manifest-path tools/smart-mixed-shadow/Cargo.toml -- -D warnings
cargo clippy --offline --locked --all-targets --no-default-features --manifest-path tools/smart-mixed-shadow/Cargo.toml -- -D warnings
cargo build --release --offline --locked --features fixtures --manifest-path tools/smart-mixed-shadow/Cargo.toml
```

The `fixtures` feature only enables the synthetic SQLite factory, fault injection budget and its tests. It is off by default. Its schema is built from **all 24 actual production SQL migration files**, using `include_str!`; those files are not edited. Production startup's one-time data reclassification is unnecessary on an empty synthetic database and is not executed.

```powershell
# Produces a new, never-overwritten database in .tmp/smart-mixed-phase2/cli-fixtures/.
tools/smart-mixed-shadow/target/release/m2shelf-smart-mixed-shadow.exe --make-fixture 3
# Use the synthetic_db returned above as <synthetic.sqlite>:
tools/smart-mixed-shadow/target/release/m2shelf-smart-mixed-shadow.exe --shadow <synthetic.sqlite> 1 --authorized-index
```

`--shadow` opens only an explicitly selected index database in SQLite READ_ONLY + query_only mode; the final switch requires the operator to acknowledge permission to read that index. It never selects an application database automatically. **This delivery did not query a personal database.** Default CLI output contains counts, rule codes and scoped hashes, with no source paths or title lists. It returns exit 2 with a bounded error code on failure and emits no partial success report. Do not use a real personal database without its owner's explicit authorization.

`adapter::ReadIndex::read` returns the single-transaction `IndexSnapshot`. `run` additionally calls the unchanged Phase 1 recognizer and compares current indexed ownership/detail projections. The full Rust objects include private indexed locators, relative names and identities: serialize only into private local output. The tests write three synthetic full reports to `.tmp/smart-mixed-phase2/*.shadow.json`. No API applies a proposal to SQLite.

Run `python tools/smart-mixed-shadow/scripts/verify.py` after recording the Rust tests in `.tmp/smart-mixed-phase2/phase1-tests.log` and `shadow-tests-final.log`; it rechecks all 49 Phase 1 CLI goldens, 11 SQLite CLI scenarios twice, and fresh-process 1k/10k/100k benchmarks three times. It writes private results plus synthetic-only `docs/smart-mixed/phase2-verification.json`. `scripts/check_scope.py` checks the existing private baseline manifest without restoring or changing any old files.

Important limitations: indexed image pages have no persistent binary-validation attestation, so their books retain exact IDs/page order as physical fallbacks. Unopened readable ResourceFiles retain ResourceFile identity and current table/count behavior; they do not become verified Works. Manual and binding fences are conservative, including equal bindings. Only Root and up to 64 initial browse anchors have full old-detail projections, keeping worst-case overlap bounded. All sources/physical entries remain covered. See the protocol and results documents before planning production integration.
