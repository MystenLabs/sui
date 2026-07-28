# TODO: Collapse RpcReader into ForkStore — COMPLETE

Plan: see tasks/plan.md

## Phase 1: Core collapse
- [x] Task 1a: store.rs: reverted `inventory()` accessor; deleted the nine
      RPC-shaped helpers; added "RPC storage traits" section with
      `impl ReadStore / RpcStateReader / RpcIndexes for ForkStore`, private
      `stock_reader()` + `chain_identifier()` helpers, `fallback_on_missing` +
      `optional_store_read`; imports added; `#[path]` test module declared
- [x] Task 1b: deleted `src/rpc/reader.rs`; dropped `mod reader;` from `rpc/mod.rs`
- [x] Task 2: startup.rs: `Arc::new(store)`, import dropped, comments fixed

## Checkpoint 1
- [x] `cargo check -p sui-fork` green

## Phase 2: Tests
- [x] Task 3a: new `src/tests/store_rpc_traits.rs` (ex-reader.rs tests)
- [x] Task 3b: tests/store_execution.rs (dropped `fork_rpc_reader`, `store.clone()`)
- [x] Task 3c: tests/rpc_executor.rs (UFCS trait calls on the store)
- [x] Task 3d: tests/subscription_e2e.rs (`Arc::new(store)`)
- [x] Task 3e: tests/store_transaction_fallback.rs (`ReadStore::get_events(&store, ..)`)
- [x] Task 3f: seed.rs tests (`RpcIndexes` imported; method calls resolve to trait)
- [x] Bonus: renamed two stale test names (`test_rpc_reads_serve_indexed_post_fork_data_from_rpc_store`,
      `test_rpc_latest_read_ignores_stale_cached_history`)

## Checkpoint 2
- [x] `cargo check -p sui-fork --all-targets` green
- [x] `SUI_SKIP_SIMTESTS=1 cargo nextest run -p sui-fork`: 109 passed, 2 skipped

## Phase 3: Polish
- [x] Task 4: `cargo fmt` + `cargo xclippy -p sui-fork` clean
- [x] Task 5: design/storage.md + design/storage_diagrams.md updated

## Checkpoint: Complete
- [x] No `RpcReader` references in code or docs
- [x] All tests pass, lint clean
