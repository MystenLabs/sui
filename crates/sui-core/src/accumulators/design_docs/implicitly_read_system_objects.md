# Implicitly-Read System Objects

This document describes the machinery that lets a transaction read a **system object during
execution without declaring it as an input** — deterministically across validators, reproducibly
when executing from effects, and safely on nodes that have not yet caught up.

Today the only such object is the **accumulator root (`0xACC`)**: the in-VM object-funds
sufficiency check reads the root (and per-account balance children under it) at a
consensus-assigned version, but user transactions never declare the root as an input — if they
did, every withdrawal would sequence-contend on one object. The machinery, however, is generic,
and this document is written for the person adding the *second* implicitly-read system object as
much as for anyone touching the first.

> Status note: the version-assignment, retry, and executor plumbing described here is on `main`.
> The effects-recording/recovery pieces and the in-VM caller land with the tail of the
> object-funds-in-VM PR stack (#27079, #27080, #27128); until those merge, the corresponding
> symbols (`IMPLICITLY_READ_SYSTEM_OBJECTS`, `accessed_consensus_objects`,
> `loaded_system_objects`) exist only on those branches.

> Terminology: recovering these reads happens whenever a node **executes from effects** —
> checkpoint execution during state sync, and crash recovery. This doc deliberately avoids the
> word "replay" for that, because `sui-replay` names an unrelated debugging tool (which is itself
> one more consumer of the effects-derived version map — see the component map below).

## The problem being solved

A read of mutable on-chain state from inside execution must satisfy three requirements that are
normally provided by declared inputs, and which we must re-provide by other means:

1. **Determinism across validators.** Every validator must read the *same version* of the object,
   or effects diverge (fork). Declared inputs get this from consensus version assignment; an
   undeclared read needs its version assigned out-of-band.
2. **Reproducibility when executing from effects.** A node executing from effects (state sync,
   checkpoint executor,
   crash recovery) has no consensus scheduler to re-derive the version. Declared inputs record
   their versions in effects; an undeclared read must record its version too.
3. **Local availability.** Version assignment tells a node *which* version to read, not that the
   node *has* it. Declared inputs are gated by the scheduler's input-dependency tracking; an
   undeclared read can race ahead of the write that produces its version, and needs a
   wait-and-retry path.

Contrast with two neighboring mechanisms that deliberately do **not** use this machinery:

- **Per-epoch config objects** (deny list, etc.): read during execution at whatever version the
  epoch fixed. Their version is *not* recorded in effects (`UnchangedConsensusKind::PerEpochConfig`
  carries no version) because the epoch itself pins the read. See invariant I9 for why this makes
  per-epoch-config objects ineligible for implicit reads.
- **Coin-reservation resolution** (`coin_reservations.md`): reads a balance child object, but only
  its *immutable* fields (owner, type), through a **≤-bounded** MVCC lookup. Any version gives the
  same answer, so it needs neither a version-exact read nor an availability gate — the eager
  address-funds scheduler's settlement gating covers existence. Use that pattern instead of this
  machinery whenever the data you read is immutable for the object's lifetime.

Rule of thumb: **this machinery is for version-exact reads of mutable state.** If your read is
version-insensitive, you do not need it and should not pay for it.

## Component map

```
                     consensus / checkpoint executor
                                  │
             ┌────────────────────┴──────────────────────┐
             │ shared_object_version_manager.rs           │
             │  - IMPLICITLY_READ_SYSTEM_OBJECTS (const,  │
             │    defined in sui-types lib.rs)            │
             │  - AssignedVersions.system_object_versions │
             │    live:  assign_versions_for_certificate  │
             │    effects: assign_versions_from_effects   │
             └────────────────────┬──────────────────────┘
                                  │ BTreeMap<ObjectID, SequenceNumber>
                                  ▼
             ┌───────────────────────────────────────────┐
             │ authority.rs (prepare_certificate)         │
             │  - passes map into the executor            │
             │  - on retry: wait_for_system_object_and_   │
             │    reenqueue → notify_read_system_object_  │
             │    at_version (writeback_cache.rs)         │
             └────────────────────┬──────────────────────┘
                                  │ Executor trait (sui-execution/src/executor.rs)
                                  ▼
             ┌───────────────────────────────────────────┐
             │ TemporaryStore (latest sui-adapter)        │
             │  - system_object_versions   (allowed map)  │
             │  - check_system_object_available (gate)    │
             │  - loaded_system_objects    (reads made)   │
             │  - retry_request            (OnceCell)     │
             └───────┬─────────────────────────┬─────────┘
                     │ effects                 │ InnerTemporaryStore.retry_request
                     ▼                         ▼
     compute_unchanged_consensus_objects   authority retry path
     (ReadOnlyRoot entries; effects_v2.rs) (ExecutionOutput::RetryLater)
                     │
                     ▼
     accessed_consensus_objects() ──► assign_versions_from_effects (checkpoint execution; loop)
```

Ownership at each boundary:

| Boundary | What crosses it | Direction |
|---|---|---|
| version manager → authority | `AssignedVersions.system_object_versions`: the versions this tx is *allowed* to read | down |
| authority → executor | the same map, verbatim (`Executor::execute_transaction_to_effects`) | down |
| executor → effects | `loaded_system_objects`: the reads *actually made*, as `ReadOnlyRoot` unchanged-consensus entries | up |
| executor → authority | `InnerTemporaryStore.retry_request`: "discard these effects and retry later" | up |
| effects → version manager | `accessed_consensus_objects()`, harvested when executing from effects to rebuild the map | loop |
| authority → object cache | `notify_read_system_object_at_version(full_id, version)`: exact-version wait | down |
| epoch start config → authority | `system_object_initial_shared_version(id)`: recovers the initial shared version needed to form the cache key | down |
| effects → `sui-replay-2` | the tool derives the same map from the transaction's expected effects (a superset harvest: every accessed consensus object at its recorded version — harmless, since only performed reads consult it; `sui-replay-2/src/execution.rs`) | loop |

Note the deliberate asymmetry: the map going **down** is what the transaction *may* read; the
recording coming **up** is what it *did* read. A transaction that never reaches the read (e.g.
aborts earlier) carries the allowance but records nothing, and that is fine — execution from effects re-runs the
transaction deterministically and will not reach the read either.

## Lifecycle of one implicit read

Using the accumulator root and the in-VM funds check as the running example:

1. **Sequencing (live).** `assign_versions_for_certificate` computes the root version at the start
   of the transaction's consensus commit (the version written by the *previous* commit's
   settlement) and stores it in `AssignedVersions::system_object_versions`. This happens for every
   transaction when accumulators are enabled, whether or not it will read the root — the version
   doubles as the input to coin-reservation rewriting and early-error construction.
2. **Dispatch.** `prepare_certificate` clones the map out of the `ExecutionEnv` and passes it
   through the `Executor` trait into the temporary store. Executors for old execution versions
   accept and ignore the parameter.
3. **The gated read.** The `check_sufficient_object_funds` native (called from
   `sui::funds_accumulator::withdraw_from_object`) reaches
   `TemporaryStore::check_system_object_available(object_id)`, which:
   - looks up the required version in `system_object_versions` — **absence is an invariant
     violation** (the transaction is reading something it was not sequenced against), not a retry;
   - loads the object at **exactly** that version — absence means this node has not caught up, so
     it records `ExecutionRetryError::SystemObjectUnavailable` in `retry_request` and returns the
     `PartialVMError` (`SYSTEM_OBJECT_NOT_AVAILABLE_LOCALLY`, status code 4034) that unwinds the
     VM, **in one expression** (see I1);
   - on success, records `(version, digest)` into `loaded_system_objects`.
4. **Retry (node-local).** The authority sees `retry_request` on the inner temporary store,
   discards the produced effects, spawns `wait_for_system_object_and_reenqueue` (an exact-version
   `notify_read` bounded by `within_alive_epoch`), and returns `ExecutionOutput::RetryLater`. Two
   metrics track this per object id: `system_object_unavailable_retries` and
   `system_object_unavailable_retry_wait_latency`.
5. **Effects recording.** On the success path, `compute_unchanged_consensus_objects` appends each
   `loaded_system_objects` entry as `UnchangedConsensusKind::ReadOnlyRoot((version, digest))` —
   unless the object is already recorded (as a changed object, e.g. the settlement transaction
   mutating the root, or as an unchanged consensus entry from a declared read-only input).
6. **Checkpoint execution.** `assign_versions_from_effects` rebuilds the map: it collects
   `accessed_consensus_objects()` (which surfaces declared-mutated inputs, declared read-only
   inputs, *and* the implicit `ReadOnlyRoot` recordings), keeps the ids in
   `IMPLICITLY_READ_SYSTEM_OBJECTS`, drops cancelled-sentinel versions, and back-fills the
   accumulator root from the out-of-band `accumulator_version` (reconstructed by the checkpoint
   executor from the settlement transaction's `object_changes`). Execution on the syncing node
   then runs step 3 against the identical map.

## Invariants

Each invariant states where it is enforced. "debug_fatal" means panic in debug/test builds, error
log in release.

**I1 — Lockstep minting.** `SYSTEM_OBJECT_NOT_AVAILABLE_LOCALLY` (4034) is minted in exactly one
place — `TemporaryStore::check_system_object_available` — in the same expression that sets
`retry_request`. Neither signal can exist without the other. Enforced by:
`enforce_retry_invariant` (execution engine, both directions, debug_fatal), the authority's hard
`assert!` that an unwind with `ExecutionErrorKind::SystemObjectNotAvailableLocally` implies a
recorded retry request, and code review — do not add a second mint site.

**I2 — Retry-requested effects are never committed.** The authority discards them and re-enqueues.
`TransactionOutputs::build_transaction_outputs` debug_fatals if a retry-requested store reaches the
commit path. Consequence: 4034 / `SystemObjectNotAvailableLocally` **never appears in committed
effects**; the SDK and proto conversions panic on it by design. If you ever see that variant in a
serialized effects structure, a node has forked from this invariant.

**I3 — Retry is node-local and invisible to consensus.** Whether a node takes the retry path
depends only on its local catch-up state; the eventually-committed effects are identical either
way. Never make committed output depend on whether a retry happened (no counters in effects, no
gas difference, nothing).

**I4 — Every in-execution read must be pre-assigned.** `check_system_object_available` treats a
lookup miss in `system_object_versions` as an invariant violation (debug_fatal +
`UNKNOWN_INVARIANT_VIOLATION_ERROR`), not a retry. Adding a new implicit read site without
extending version assignment will fail loudly and deterministically on every node.

**I5 — Required versions sit at the frontier and are never pruned.** The assigned version is at
most one settlement behind the live object. Local absence therefore always means "not yet
committed here", never "pruned"; the retry wait is guaranteed to terminate (or be cancelled by
epoch change). If you assign versions that can lag arbitrarily (e.g. deep-historical reads),
this reasoning breaks — don't.

**I6 — Reads are recorded at the required version, not the latest.** `loaded_system_objects`
stores the version and digest *as of the assigned version*, so the recorded effects entry is
identical on every node regardless of how far the object has since advanced.

**I7 — Membership means "may be read implicitly", nothing more.** An object in
`IMPLICITLY_READ_SYSTEM_OBJECTS` can still be declared as an explicit input by any transaction
(the settlement transaction mutates the root; a user transaction may pass it). Declared handling
is unchanged and takes its normal path; the implicit-read map is populated *in addition*
(the recovery harvest keeps the id even when it was also a declared input). Never write code that
assumes an implicit-read object cannot appear among declared inputs, or vice versa.

**I8 — Members must be registered in the epoch start config and exist at epoch start.**
The retry path recovers the object's initial shared version via
`EpochStartConfiguration::system_object_initial_shared_version` (backed by
`SYSTEM_SHARED_OBJECT_IDS`) and `expect`s it. An implicitly-read object created mid-epoch would
panic here rather than retry. The accumulator root satisfies this from the epoch in which
accumulators are enabled onward.

**I9 — Members must not be per-epoch-config-read objects.** The effects dedup skips appending a
`ReadOnlyRoot` when the id is already recorded — but a `PerEpochConfig` entry carries **no
version**, so the read version could not be recovered from effects for such an object. Today's member (the
root) is unaffected; check this before extending the set.

**I10 — Members must have a well-defined version to assign for every transaction.** The
accumulator root qualifies because the settlement barrier writes it every commit, so "version at
the start of this commit" always exists. An object written only sporadically has no such anchor;
you would need a different assignment rule and must think through what can be recovered from effects when no
transaction in a checkpoint touched the object.

**I11 — `accessed_consensus_objects()` includes implicit reads.** The accessor (renamed from
`input_consensus_objects` for this reason) returns every consensus object the transaction read or
wrote — declared or not — and an implicit read is *indistinguishable in effects* from a declared
read-only shared input. Consumers that need "declared inputs only" must filter against the
transaction's own input set (`initial_version_map` in the version manager is the model). Audit any
new consumer for this; the existing ones (causal order, congestion tracker, JSON-RPC
`shared_objects`, the `sui-replay` tool) have been audited and either want the implicit entries or are
insensitive to them.

**I12 — Closure check.** `assign_versions_from_effects` debug_asserts that every accessed
consensus object is either a declared input or a member of `IMPLICITLY_READ_SYSTEM_OBJECTS`.
This is the tripwire for "someone added an implicit read without adding the object to the const" —
it fires on the checkpoint-execution path in every debug/simtest run.

**I13 — Cancelled sentinels are not read versions.** A cancelled transaction records special
versions (`CANCELLED_READ`, `CONGESTED`, …) for its declared inputs. The recovery harvest filters
them (`!version.is_cancelled()`) so a sentinel neither enters `system_object_versions` nor blocks
the accumulator back-fill. Preserve this filter if you touch the harvest.

**I14 — The availability wait is exact-version.** `notify_read_system_object_at_version` is keyed
on `InputKey::VersionedObject { id, version }` — the exact version — and is correct only because
assigned system-object versions are always eventually written by some transaction. Do not call it
with a version that might never be written (it would hang until epoch end), and do not assume ≥
semantics.

**I15 — Retries are bounded and monotonic.** Versions are fixed at sequencing; system-object
availability only grows; execution unwinds on the first unavailable object. Worst case is one
retry per distinct implicitly-read object the transaction touches. A hard retry cap is therefore
unnecessary (and would convert a slow-node latency into a spurious failure). If retry counts ever
look pathological in the metrics, the node is far behind — fix catch-up, not the retry logic.

**I16 — A framework native must be registered in every execution version that links the current
framework.** The framework bytecode published at genesis is always built from current sources, but
tests (and potentially nodes) construct genesis at old protocol versions, which resolve natives
against **old execution versions'** tables. A native declared in a framework module but missing
from a table fails module linking with `MISSING_DEPENDENCY` at genesis. Concretely:
`check_sufficient_object_funds` is registered in `latest` (real implementation) **and** `v3`
(documented no-op — the gating feature flag can never be on at protocol versions that run v3).
Follow the same pattern for any new native this machinery grows: real impl in `latest`, no-op in
every older execution version whose protocol range can genesis with the current framework
(v3 today; v0–v2 do not register the sibling `funds_accumulator` natives either, because no
supported genesis path links the module under them).

**I17 — The accumulator root back-fill is unconditional.** The root's version is populated in
`system_object_versions` whenever accumulators are enabled — even for transactions that never read
it — because non-read consumers (coin-reservation rewriting, early-error construction) take it
from `AssignedVersions::accumulator_version()`. When adding a second member, do not "simplify" the
back-fill away; conversely, new members do *not* get a back-fill unless something outside
execution needs their version for every transaction.

## Things to be aware of when touching adjacent code

- **`prepare_certificate` ordering.** The retry check runs immediately after execution and before
  the effects-kind assert and the object-funds post-execution checker. Keep it first: everything
  downstream assumes retry-requested effects have already been filtered out.
- **Epoch change during a wait.** The re-enqueue task is wrapped in `within_alive_epoch`; a
  reconfiguration cancels it and the transaction is recovered by the new epoch's machinery. The
  wait-latency metric records only completed waits, on purpose.
- **`new_for_testing`.** `AssignedVersions::new_for_testing(shared, accumulator_version)` is the
  test-only accumulator-root sugar; production call sites build the map explicitly. If your test
  needs a second system object, build the map — don't extend the sugar.
- **Dev-inspect / dry-run.** Simulation paths construct the map outside consensus: the executor
  pins every member of `IMPLICITLY_READ_SYSTEM_OBJECTS` at its latest committed version
  (`sui-execution/src/latest.rs`), and the simulate path filters those pinning reads out of
  `unchangedLoadedRuntimeObjects` (`authority.rs`) — a new member gets both automatically. Confirm
  latest-committed is the right version anchor for your object, and note the availability gate does
  not apply (simulation has no retry loop).
- **Snapshot surfaces.** `ExecutionErrorKind::SystemObjectNotAvailableLocally` is BCS variant 42;
  it exists in `exec_failure_status.yaml` / `format__sui.yaml.snap` like any other variant, but per
  I2 it must never be produced into committed effects. Appending future variants goes after it as
  usual.

## Checklist: adding a new implicitly-read system object

1. Add the `ObjectID` to `IMPLICITLY_READ_SYSTEM_OBJECTS`
   (`crates/sui-types/src/lib.rs`; it lives in `sui-types` so the execution layer can see it).
   Re-read I7–I10 and confirm each holds for the new object:
   not per-epoch-config-read, registered in `SYSTEM_SHARED_OBJECT_IDS`, exists at epoch start, has
   a well-defined per-transaction version anchor.
2. Extend **live version assignment** (`assign_versions_for_certificate`) to fold the object's
   version into the map for every transaction that may read it. If the version comes from
   somewhere other than `shared_input_next_versions`, document the anchor.
3. Confirm **recovery from effects** needs no change: if the object appears in effects when read
   (which I5/I6 + the effects recording guarantee), the generic harvests pick it up — both
   `assign_versions_from_effects` and the `sui-replay-2` tool's superset harvest
   (`sui-replay-2/src/execution.rs`). Only a
   back-fill-style consumer (I17) needs new code.
4. Implement the **read site** behind `check_system_object_available` — never read the object's
   state without the gate, and propagate its error with `?` untouched (I1).
5. If the read is reached from Move, register the **native in every execution version** per I16.
6. Gate behavior changes behind a **protocol feature flag** (`/protocol-config` skill), since the
   recorded `ReadOnlyRoot` entries change effects digests for transactions that take the read.
7. Tests: an e2e that exercises the lagging-node retry (see the object-withdraw stress test), and
   an effects-equivalence check that the recorded read reproduces the version.

## Related documents

- [`object_funds_checking.md`](./object_funds_checking.md) — the post-execution sufficiency
  checker that the in-VM check supersedes when its feature flag is on.
- [`write_path.md`](./write_path.md) — settlement transactions and the barrier that makes the
  accumulator root's per-commit version anchor exist.
- [`coin_reservations.md`](./coin_reservations.md) — the version-insensitive bounded-read pattern
  that does *not* need this machinery.
