# Move VM Bytecode Profiling

Opcode-level execution profiling for the Move VM interpreter. Counts how many
times each opcode is dispatched so you can:

- Identify hot opcodes for dispatch-loop optimization
- Sanity-check gas calibration against observed frequency
- Compare distributions across workloads (benchmarks vs. mainnet replay)

## Enabling

Profiling lives behind the `tracing` feature. Build or run with:

```bash
cargo build --features move-vm-runtime/tracing
cargo test  --features move-vm-runtime/tracing -- test_profiling
```

When the feature is disabled the increment in the dispatch hot loop compiles
out entirely — zero runtime cost.

## Scope

Counters are **per `MoveRuntime`**. A process with two runtimes has two
independent counter sets; snapshots from one cannot see increments from the
other. This matters for concurrent replay and for test isolation.

## Reading the data

### Via the telemetry API (in-process)

```rust
use move_vm_runtime::runtime::MoveRuntime;

let runtime: MoveRuntime = /* ... */;
// run transactions ...
let report = runtime.get_telemetry_report();
let stats = &report.bytecode_stats;
println!("total instructions: {}", stats.total());
println!("{}", stats.format_report());
```

### Via the env-var dump

Set `MOVE_VM_DUMP_PROFILE_FILE` to a path, then call
`MoveRuntime::emit_bytecode_profile()` (or, at the sui-execution layer,
`Executor::emit_bytecode_profile()`). The full snapshot is written as JSON to
that path; the summary is also logged via `tracing::info!`.

```bash
MOVE_VM_DUMP_PROFILE_FILE=/tmp/profile.json \
  cargo run --features tracing -p sui-replay-2 -- <args>
```

### Replay-driven profiling

`sui-replay-2` calls `Executor::emit_bytecode_profile()` after each transaction
when built with `--features tracing`. Point `MOVE_VM_DUMP_PROFILE_FILE` at a
path and run a replay to get a per-transaction dump.

## Output formats

### CSV (`BytecodeSnapshot::format_csv`)

```
opcode,count,percentage
ADD,5023,31.4502
LT,3100,19.4060
RET,2000,12.5196
...
```

### JSON (`BytecodeSnapshot::format_json`)

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

### Human report (`BytecodeSnapshot::format_report`)

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

## Cost

- **Feature off** — single `#[cfg]`-gated line in `step`; no code emitted.
- **Feature on** — one `HashMap::get` + `AtomicU64::fetch_add(Relaxed)` per
  dispatched instruction. Relaxed ordering; no fences, no contention with
  non-profiling paths.

The counter map is sized at `MoveRuntime` construction and never resized, so
the hot path does no allocation.

## Extending

Adding a new opcode variant: extend the `ALL_OPCODES` list in
`counters.rs`. The list is the authoritative set of opcodes tracked; a
variant missing from the list will silently drop its increments.
