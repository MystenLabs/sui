# Bytecode Profiling Infrastructure

This module provides bytecode execution frequency counters for profile-guided optimization (PGO) of the Move VM interpreter.

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

## Future Work

- Integration with telemetry reporting
- Hot function identification
- Branch taken/not-taken ratio tracking
- Instruction sequence (n-gram) analysis
