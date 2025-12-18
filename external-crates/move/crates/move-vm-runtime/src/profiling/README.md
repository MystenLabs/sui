# Bytecode Profiling Infrastructure

This module provides bytecode execution frequency counters for profile-guided optimization (PGO) of the Move VM interpreter.

## Advantages of Instruction Profiling

Instruction profiling provides insights for optimizing interpreter performance:

### 1. Data-Driven Optimization Decisions

- **Validate optimization effectiveness**: Profile data reveals actual instruction frequencies from real workloads. We can then compare before/after profiles to measure actual improvement.

### 2. Dispatch Loop Optimization

- **Match arm reordering**: Place the most frequent opcodes first in the dispatch switch/match statement. Modern CPUs predict forward branches as not-taken, so placing hot cases first reduces branch mispredictions

### 3. Superinstruction Design

- **Identify common sequences**: Profile data combined with trace analysis reveals frequently occurring instruction pairs or sequences (e.g., COPY_LOC followed by CALL)
- **Fuse hot paths**: Create specialized superinstructions that handle common sequences in a single dispatch, reducing interpreter overhead
- **Measure fusion benefits**: Compare profiles before and after superinstruction implementation

### 4. Inline Caching and Specialization

- **Type specialization**: Identify which instructions would benefit from type-specialized fast paths
- **Inline caching candidates**: Determine which CALL sites are monomorphic and suitable for inline caching
- **Polymorphic inline caches**: Profile data guides PIC slot allocation based on actual call-site behavior

### 5. Memory Layout Optimization

- **Hot/cold code separation**: Place frequently executed instruction handlers in contiguous memory to improve instruction cache locality
- **Prefetching hints**: Profile data informs where to insert prefetch instructions for the next likely opcode
- **Branch target alignment**: Align hot instruction handlers to cache line boundaries

### 6. Workload Characterization

- **Contract analysis**: Understand which operations dominate specific smart contracts
- **Benchmark validation**: Ensure synthetic benchmarks reflect production instruction distributions
- **Regression detection**: Detect unexpected changes in instruction profiles that might indicate bugs or performance regressions

## Overview

The profiling infrastructure tracks how often each bytecode instruction is executed. This data can be used to:

- Identify hot instructions for dispatch optimization
- Reorder match arms by frequency
- Implement fast-path dispatch for common instructions
- Guide superinstruction design

## Usage

### Enabling Profiling

Add the `profiling` feature flag when building:

```bash
cargo build -p move-vm-runtime --features profiling
```

### Collecting Profile Data

When the `profiling` feature is enabled, every instruction executed in the interpreter increments the corresponding counter automatically.

```rust
use move_vm_runtime::profiling::BYTECODE_COUNTERS;

// After running workloads...

// Take a snapshot of current counts
let snapshot = BYTECODE_COUNTERS.snapshot();

// Get total instruction count
println!("Total instructions: {}", snapshot.total());

// Get a formatted report
println!("{}", snapshot.format_report());

// Get opcodes sorted by frequency (highest first)
for (opcode, count) in snapshot.sorted_by_frequency() {
    println!("{:?}: {}", opcode, count);
}

// Reset counters for next measurement
BYTECODE_COUNTERS.reset();
```

### Example Output

```
Total instructions: 1000000

Opcode                          Count         %
------------------------------------------------
COPY_LOC                       250000    25.00%
MOVE_LOC                       180000    18.00%
ST_LOC                         150000    15.00%
CALL                           100000    10.00%
...
```

## Design

### Zero Overhead When Disabled

The profiling code is conditionally compiled with `#[cfg(feature = "profiling")]`. When the feature is not enabled, there is zero runtime overhead.

### Minimal Overhead When Enabled

- Uses `AtomicU64` with `Ordering::Relaxed` for minimal synchronization cost
- Counter increment is marked `#[inline(always)]`
- Global static avoids passing profiler through the call stack

### Thread Safety

The `BytecodeCounters` struct uses atomic operations and is safe to use from multiple threads. Individual counter reads/writes are atomic, though a `snapshot()` captures counters sequentially (not as an atomic group).

## API Reference

### `BYTECODE_COUNTERS`

Global static instance of `BytecodeCounters`. Use this for all profiling operations.

### `BytecodeCounters`

- `increment(opcode: Opcodes)` - Increment counter for an opcode
- `get(opcode: Opcodes) -> u64` - Get current count for an opcode
- `snapshot() -> BytecodeSnapshot` - Capture point-in-time counts
- `reset()` - Reset all counters to zero

### `BytecodeSnapshot`

- `get(opcode: Opcodes) -> u64` - Get count for an opcode
- `total() -> u64` - Get total instruction count
- `sorted_by_frequency() -> Vec<(Opcodes, u64)>` - Get opcodes sorted by count
- `format_report() -> String` - Generate human-readable report

## Sui Integration

### sui-replay-2

The `sui-replay-2` crate integrates profiling to capture bytecode execution profiles for replayed transactions.

#### Enabling Profiling in sui-replay-2

```bash
cargo build -p sui-replay-2 --features profiling
```

#### Feature Propagation

The profiling feature propagates through the crate hierarchy:

```
sui-replay-2 --features profiling
    └── sui-execution --features profiling
            └── move-vm-runtime-latest --features profiling
```

#### Accessing Profile Data

When profiling is enabled, the `TxnContextAndEffects` struct includes a `bytecode_profile` field:

```rust
pub struct TxnContextAndEffects {
    // ... other fields ...
    #[cfg(feature = "profiling")]
    pub bytecode_profile: sui_execution::profiling::BytecodeSnapshot,
}
```

The profile is captured automatically after each transaction execution in `execute_transaction_to_effects()`. Counters are reset before each transaction, so the snapshot represents only that transaction's execution.

#### Example Usage

```rust
use sui_replay_2::execution::execute_transaction_to_effects;

let (result, context) = execute_transaction_to_effects(txn, epoch_store, object_store, &mut None)?;

#[cfg(feature = "profiling")]
{
    println!("Transaction bytecode profile:");
    println!("{}", context.bytecode_profile.format_report());
}
```

## Future Work

- Integration with telemetry reporting
- Hot function identification
- Branch taken/not-taken ratio tracking
- Instruction sequence (n-gram) analysis
