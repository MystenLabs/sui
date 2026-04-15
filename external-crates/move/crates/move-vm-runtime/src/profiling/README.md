# Move VM Bytecode Profiling

Opcode-level execution profiling for the Move VM interpreter. Counts how many
times each opcode is dispatched so you can:

- Identify hot opcodes for dispatch-loop optimization
- Sanity-check gas calibration against observed frequency
- Compare distributions across workloads (benchmarks vs. mainnet replay)

## Enabling

Profiling lives behind the `tracing` feature. `move-vm-runtime` is in the
`external-crates/move` workspace, so `-p move-vm-runtime` commands must be
run from inside that workspace. Commands for `sui-replay-2` run from the
top-level sui workspace.

```bash
# Build the runtime with profiling enabled
(cd external-crates/move && cargo build -p move-vm-runtime --features tracing)

# Run only the profiling tests
(cd external-crates/move && cargo test -p move-vm-runtime --features tracing -- profiling)

# Replay a transaction with profiling enabled (from the sui workspace)
cargo run --features tracing -p sui-replay-2 -- <args>
```

When the feature is disabled the increment in the dispatch hot loop compiles
out entirely — zero runtime cost.

## Scope

Counters are **per `MoveRuntime`**. A process with two runtimes has two
independent counter sets; snapshots from one cannot see increments from the
other. This matters for:

- Concurrent replay sessions
- Test isolation (each test adapter creates its own runtime)
- Validators that spawn one runtime per execution-layer version

Within a single runtime, counters are atomic and may be incremented from
multiple threads concurrently. Counts merge at `Relaxed` ordering — fine for
frequency profiling, not suitable for ordering or causality reasoning.

## API surface

All APIs are gated by the `tracing` feature unless noted.

### On `MoveRuntime`

| Method | Purpose |
|--------|---------|
| `bytecode_profile_snapshot()` | Take a `BytecodeSnapshot` without emitting anything. |
| `emit_bytecode_profile()` | Log a CSV summary via `tracing::info!` and, if `MOVE_VM_DUMP_PROFILE_FILE` is set, write JSON to that path. |
| `reset_bytecode_profile()` | Zero out all counters. Used to measure per-transaction (rather than cumulative) stats. |
| `get_telemetry_report()` | Returns `MoveRuntimeTelemetry` whose `bytecode_stats` field contains the same snapshot. (Always available; `bytecode_stats` is `cfg(feature = "tracing")`.) |

### On `sui_execution::Executor`

The same three methods (`emit_bytecode_profile`, `reset_bytecode_profile`,
`bytecode_profile_snapshot`) are available on the cross-version `Executor`
trait. They are no-ops on the older v0/v1/v2/v3 executors and only do
meaningful work on `latest`. `bytecode_profile_snapshot` returns
`Option<BytecodeSnapshot>` for that reason.

### On `BytecodeSnapshot`

| Method | Output |
|--------|--------|
| `total()` | Total instruction count across all opcodes. |
| `get(opcode)` | Count for a single opcode. |
| `iter()` | Yields `(Opcodes, count)` for every non-zero opcode. |
| `format_report()` | Human-readable, sorted by count descending. |
| `format_csv()` | CSV with header. |
| `format_json()` | JSON suitable for analysis tools. |
| `dump_to_file(path)` | Writes the JSON to `path`. Errors logged via `tracing::warn!`, never propagated. |
| `maybe_dump_to_env_file()` | If `MOVE_VM_DUMP_PROFILE_FILE` is set, calls `dump_to_file` with that path. Otherwise no-op. |

## Reading the data

### In-process via the telemetry API

```rust
use move_vm_runtime::runtime::MoveRuntime;

# fn make_runtime() -> MoveRuntime { unimplemented!() }
let runtime = make_runtime();
// run transactions ...
let report = runtime.get_telemetry_report();
let stats = &report.bytecode_stats;
println!("total instructions: {}", stats.total());
println!("{}", stats.format_report());
```

### Per-transaction measurement

```rust
runtime.reset_bytecode_profile();
// run transaction ...
let snap = runtime.bytecode_profile_snapshot();
println!("{} instructions in this txn", snap.total());
```

### Tracing + env-file dump (one shot)

```rust
// Logs CSV via tracing::info! AND, if MOVE_VM_DUMP_PROFILE_FILE is set,
// writes JSON to that path.
runtime.emit_bytecode_profile();
```

### Replay-driven profiling

`sui-replay-2` invokes the profiling hooks once per transaction (and once at
session end). The dumping policy is controlled by `MOVE_VM_PROFILE_MODE`:

| Value | Behaviour |
|-------|-----------|
| `per-transaction` (aliases: `per_transaction`, `pertx`) | Reset before each tx, emit after. The dump file is overwritten on every tx (final file = last tx). |
| `per-transaction-file` (aliases: `per_transaction_file`, `pertxfile`) | Reset before each tx, write per-tx snapshot to `<base>.<digest>.json` (one file per tx). |
| `end-of-replay` (aliases: `end_of_replay`, `end`, `session`; default) | Accumulate across the whole run, emit once at the end. |

Unrecognised values are logged at `warn` level and fall back to the default.

#### Caveats

- **Execution version**: profiling overrides only exist on the `latest`
  execution layer. Transactions that ran on older execution versions
  (v0/v1/v2/v3) hit the default no-op `Executor::emit_bytecode_profile`
  and produce no output, even with the env vars set. Pick a recent
  transaction (or `--digest` whose protocol version uses `latest`) to
  exercise the profiler.
- **`end-of-replay` requires `--cache-executor`**. Without executor caching,
  each transaction creates and drops its own executor, so counters cannot
  survive across transactions and the session-end hook walks an empty
  cache. The per-transaction modes work in either configuration.

#### Worked example

Replay a single recent mainnet transaction and dump its bytecode profile
to `/tmp/profile.json`:

```bash
MOVE_VM_DUMP_PROFILE_FILE=/tmp/profile.json \
  cargo run --features tracing -p sui-replay-2 -- \
    --digest 29rv278vQ2WrKp5CuemUQ5EAKBiYpAR84MHFypyMCkM3 \
    --cache-executor --trace --overwrite
```

Sample output (`/tmp/profile.json`) — counts from the small system
transaction above:

```json
{
  "total": 13,
  "opcodes": [
    { "opcode": "MOVE_LOC", "count": 3, "percentage": 23.0769 },
    { "opcode": "CALL", "count": 2, "percentage": 15.3846 },
    { "opcode": "RET", "count": 2, "percentage": 15.3846 },
    { "opcode": "EQ", "count": 1, "percentage": 7.6923 },
    { "opcode": "BRANCH", "count": 1, "percentage": 7.6923 },
    { "opcode": "BR_FALSE", "count": 1, "percentage": 7.6923 },
    { "opcode": "LD_CONST", "count": 1, "percentage": 7.6923 },
    { "opcode": "WRITE_REF", "count": 1, "percentage": 7.6923 },
    { "opcode": "MUT_BORROW_FIELD", "count": 1, "percentage": 7.6923 }
  ]
}
```

## Output formats

### CSV (`format_csv`)

```
opcode,count,percentage
ADD,5023,31.4502
LT,3100,19.4060
RET,2000,12.5196
...
```

### JSON (`format_json`)

```json
{
  "total": 15965,
  "opcodes": [
    { "opcode": "ADD", "count": 5023, "percentage": 31.4502 },
    { "opcode": "LT",  "count": 3100, "percentage": 19.4060 },
    { "opcode": "RET", "count": 2000, "percentage": 12.5196 }
  ]
}
```

The JSON writer is hand-rolled (no `serde` dependency) because this crate is
infrastructure-level. The schema is fixed and small; if you need a more
elaborate format, parse this and reshape downstream.

### Human report (`format_report`)

```
Total instructions: 15965

Opcode                          Count         %
------------------------------------------------
ADD                              5023    31.45%
LT                               3100    19.41%
RET                              2000    12.52%
...
```

Rows are sorted by count descending in every format; zero-count opcodes are
omitted from the output.

## Cost model

- **Feature off** — the increment in the dispatch hot loop is behind
  `#[cfg(feature = "tracing")]`; no code is emitted.
- **Feature on** — per dispatched instruction, one `HashMap::get` followed
  by an `AtomicU64::fetch_add(Relaxed)`. No fences, no contention with
  non-profiling code paths. The counter map is sized at `MoveRuntime`
  construction and never resized, so the hot path performs no allocation.

The cost has not been benchmarked end-to-end; treat the "near-zero overhead
when on" claim as a design intent rather than a measured result. A
microbenchmark of the dispatch loop with feature on/off would be a good
follow-up before relying on profiling in production.

## Environment variables

| Variable | Effect |
|----------|--------|
| `MOVE_VM_DUMP_PROFILE_FILE` | Path that `emit_bytecode_profile` and `maybe_dump_to_env_file` write JSON to. Unset = no file dump (still logs via `tracing::info!`). |
| `MOVE_VM_PROFILE_MODE` | Replay-time policy (see the table above). Unset = `end-of-replay`. |

## Tests

| Test | What it covers |
|------|----------------|
| `profiling::counters::tests::*` | Unit-level: increment/snapshot/reset, CSV/JSON/report formatting, dump-to-file success and failure paths, per-counter independence. |
| `unit_tests::profiling_tests::*` | End-to-end through `InMemoryTestAdapter`: counts on real workloads, per-runtime isolation, telemetry-API round-trip, dump-to-file matches in-memory snapshot, CSV/JSON consistency. |
| `sui_replay_2::profiling::tests::*` | Replay-side dispatch: each `BytecodeProfileMode` calls the right hooks at the right boundaries; per-tx-path filename formatting. |

Run profiling tests:

```bash
# move-vm-runtime (runs from the move workspace)
(cd external-crates/move && cargo test -p move-vm-runtime --features tracing -- profiling)

# sui-replay-2 (runs from the sui workspace)
cargo test -p sui-replay-2 --features tracing -- profiling
```

## Extending

Adding a new opcode variant: extend the `ALL_OPCODES` list in `counters.rs`.
That list is the authoritative set of opcodes tracked; a variant missing
from the list will silently drop its increments. (No assertion enforces
exhaustiveness today — the cost is silent under-counting, not a crash, so it
won't fail loudly in tests if you forget.)
