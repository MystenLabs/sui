# Implementation Plan: Collapse RpcReader into ForkStore

## Overview

Delete the `RpcReader` wrapper struct and its file, and implement the upstream RPC
storage traits (`ReadStore`, `RpcStateReader`, `RpcIndexes`) directly on
`ForkStore` in store.rs. The routing table — which read surfaces resolve through
fork policy vs. the stock rpc-store reader — survives as a dedicated "RPC storage
traits" section with the same per-method rationale comments. `ObjectStore` and
`RuntimeObjectResolver` (the other two traits `RpcStateReader` requires) already
exist on `ForkStore`; `RpcReader`'s copies were pure forwarding and disappear.

## Architecture Decisions

- **Everything lands in store.rs; `rpc/reader.rs` is deleted.** All of
  `ForkStore`'s other trait impls already live in store.rs, impls in the same
  module reach `self.inner.inventory` without any `pub(crate)` accessor leaks,
  and `rpc/` is left holding only genuine gRPC glue. store.rs grows to ~1350
  lines but its tests stay external via the existing `#[path]` pattern; the
  reader's tests move to a new `src/tests/store_rpc_traits.rs`.
- **The nine leaked RPC-shaped helpers on `ForkStore` are deleted** and their
  bodies become the trait-impl bodies: `latest_checkpoint_for_rpc`,
  `highest_synced_checkpoint_for_rpc`, `highest_indexed_checkpoint_seq_number`,
  `chain_identifier` (becomes a private helper), `owned_objects_iter`,
  `dynamic_field_iter`, `coin_info`, `balance`, `balance_iter`. This removes
  every identical-signature name collision between inherent methods and the new
  trait methods. The `inventory()` accessor added earlier is reverted — no
  longer needed once the impls share the module.
- **`to_storage_error` stays private to store.rs**; reader.rs's duplicate dies
  with the file.
- **Name-collision policy**: inherent methods (`get_transaction`,
  `get_checkpoint_by_sequence_number`, …, returning `anyhow::Result`) keep their
  names; trait impls call them via `ForkStore::method(self, ..)` UFCS, which
  resolves to the inherent method (inherent always wins) — the pattern the
  SimulatorStore impl in the same file already uses. Verified no recursion
  hazard and no coherence conflict (simulacrum's blanket `ReadStore` impl is on
  `Simulacrum<T, V>`, not on the store type).
- **`use_store_on_missing` renames to `fallback_on_missing`** — "store" is
  ambiguous once the impls live on the store itself.
- Behavior is preserved exactly: same routing, same error-degradation
  conventions (`optional_store_read` log-and-`None` on the RPC surface), same
  hybrid reads (chain identifier, highest indexed checkpoint).

## Task List

### Phase 1: Core collapse (one atomic compile unit)
- [ ] Task 1: Move trait impls into store.rs; delete `rpc/reader.rs`; strip the
      nine helpers; revert `inventory()` accessor; update `rpc/mod.rs`
- [ ] Task 2: Update production call sites (`startup.rs`)

### Checkpoint: lib compiles
- [ ] `cargo check -p sui-fork` green (lib target)

### Phase 2: Tests
- [ ] Task 3: New `src/tests/store_rpc_traits.rs` (ex-reader tests) + update test
      call sites (store_execution.rs, rpc_executor.rs, subscription_e2e.rs,
      store_transaction_fallback.rs, seed.rs)

### Checkpoint: tests compile and pass
- [ ] `cargo check -p sui-fork --all-targets` green
- [ ] `SUI_SKIP_SIMTESTS=1 cargo nextest run -p sui-fork` green

### Phase 3: Polish
- [ ] Task 4: Lint (`cargo fmt`, `cargo xclippy -p sui-fork`)
- [ ] Task 5: Update design docs (storage.md, storage_diagrams.md)

## Task Details

### Task 1: Move impls into store.rs, delete reader.rs

**Description:** Add an "RPC storage traits" banner section to store.rs holding:
a private inherent block (`stock_reader()`, `chain_identifier()`), `impl
ReadStore for ForkStore`, `impl RpcStateReader for ForkStore`, `impl RpcIndexes
for ForkStore`, and the `fallback_on_missing` / `optional_store_read` helpers —
all bodies moved verbatim from reader.rs with `self.store` → `self`. Delete the
nine RPC-shaped inherent helpers whose bodies moved. Extend the section header
with the load-bearing rationale (stock reverse-scan over the sparse `objects` CF
must not serve latest reads). Add the missing imports (`MoveTypeLayout`,
`RpcStoreReader`, `ObjectSet`, `EpochInfo`, `ObjectKey`, `Ledger*`,
`VersionedFullCheckpointContents`, `RpcStateReader`). Delete `src/rpc/reader.rs`
and its `mod` declaration in `rpc/mod.rs`. Note the ForkStore struct doc should
mention it backs the RPC service directly.

**Acceptance criteria:**
- [ ] `RpcReader` and `rpc/reader.rs` no longer exist; `ForkStore` implements
      the three traits in store.rs
- [ ] Routing unchanged: same stock-only, fork-policy, and hybrid method sets
- [ ] No identical-signature inherent/trait method pairs remain; no new
      `pub(crate)` surface added

**Verification:** `cargo check -p sui-fork` (lib; test targets fixed in Task 3)

**Dependencies:** None
**Files:** `src/store.rs`, `src/rpc/reader.rs` (deleted), `src/rpc/mod.rs`
**Estimated scope:** Medium (the bulk of the change)

### Task 2: Update `startup.rs`

**Description:** `Arc::new(RpcReader::new(store))` → `Arc::new(store)`; drop the
`RpcReader` import; update the two comments describing the reader
(`startup.rs:223-224`, `startup.rs:237-239`). `resume_base_checkpoint`'s
`store.get_highest_verified_checkpoint()` keeps resolving to the inherent
method — no change there.

**Verification:** `cargo check -p sui-fork` green
**Dependencies:** Task 1
**Files:** `src/startup.rs`
**Estimated scope:** XS

### Task 3: Tests

**Description:** Method-call syntax that previously hit `RpcReader`'s trait
methods would now resolve to `ForkStore`'s inherent methods (different return
types — compile errors, by design), so those call sites become explicit trait
calls:
- New `src/tests/store_rpc_traits.rs` (declared `#[path]` from store.rs):
  carries over reader.rs's tests — the `fallback_on_missing` unit tests
  (renamed) and `ledger_indexes_delegate_to_rpc_store` using the store directly.
- `src/tests/store_execution.rs`: delete `fork_rpc_reader`; `let reader =
  store.clone()` (call sites already UFCS).
- `src/tests/rpc_executor.rs` (~481): use the store; rewrite the four
  method-syntax reads as `ReadStore::…` / `ObjectStore::…` UFCS calls.
- `src/tests/subscription_e2e.rs` (109): `Arc::new(store)`.
- `src/tests/store_transaction_fallback.rs` (~363): drop the binding;
  `ReadStore::get_events(&fallback_store, ..)`.
- `src/seed.rs` tests (~815-828): `RpcIndexes::owned_objects_iter(&store, ..)`.

**Acceptance criteria:**
- [ ] No `RpcReader` references anywhere in the crate
- [ ] Tests exercise the same trait surfaces as before (trait-path calls, not
      inherent fallbacks)

**Verification:** `cargo check -p sui-fork --all-targets`, then
`SUI_SKIP_SIMTESTS=1 cargo nextest run -p sui-fork`
**Dependencies:** Task 1
**Files:** 6 test-bearing files
**Estimated scope:** Medium (mechanical)

### Task 4: Lint

**Verification:** `cargo fmt --all` (scoped diff), `cargo xclippy -p sui-fork`
**Dependencies:** Tasks 1-3

### Task 5: Update design docs

**Description:** `design/storage.md` ("Where reads resolve", flow listing ~line
20) and `design/storage_diagrams.md` (consumer diagram, routing diagram, prose
lines 18-63) describe `RpcReader` as a separate router. Rewrite: the router is
`ForkStore`'s RPC trait impls; RpcService and Simulacrum both stand directly on
`ForkStore`. storage_diagrams.md has uncommitted local modifications — read
fully first and preserve unrelated changes.

**Verification:** Manual read-through; mermaid still renders
**Dependencies:** Tasks 1-3
**Files:** `design/storage.md`, `design/storage_diagrams.md`
**Estimated scope:** Small

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Accidental recursion: trait method calling same-named inherent method | High (stack overflow at runtime) | Inherent methods always win `ForkStore::name(self, ..)` resolution; the nine identical-signature helpers are deleted outright so no identical pairs remain. Phase 2 tests exercise every surface. |
| Method-syntax ambiguity now that SimulatorStore and ReadStore share names on one type | Medium (compile-time only) | Every shared name also has an inherent method, which always wins; trait callers already use UFCS. Compiler flags any residue. |
| Behavior drift on RPC error conventions | Medium | Bodies are moved, not rewritten; log-and-`None` conventions kept verbatim. |
| storage_diagrams.md has uncommitted edits | Low | Read before editing; touch only RpcReader-related passages. |

## Open Questions

None blocking.
