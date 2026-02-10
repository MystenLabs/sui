# Transaction Execution Trace Log - Implementation Plan

## Overview
Efficient binary logging system for recording transaction execution timing in the Sui blockchain. The system uses BCS (Binary Canonical Serialization) with delta-time encoding to minimize storage overhead while capturing detailed execution traces.

## Design Goals
1. **Compactness**: Binary format with efficient time encoding
2. **Performance**: Minimal overhead on transaction execution
3. **Extensibility**: Easy to add new event types
4. **Analyzability**: Support post-hoc analysis of execution timings

## Core Data Structures

### LogRecord Enum
Main enum serialized with BCS to the log file:

```rust
#[derive(Serialize, Deserialize)]
enum LogRecord {
    /// Absolute timestamp anchor point (wall-clock time for log interpretation)
    AbsTime(SystemTime),

    /// Delta time in microseconds (0-65535 µs = 0-65.5ms)
    DeltaTime(u16),

    /// Large delta for gaps > 65.5ms
    DeltaTimeLarge(Duration),

    /// Transaction event with digest and event type
    TransactionEvent {
        digest: TransactionDigest,
        event_type: TxEventType,
    },
}
```

### TxEventType Enum
Event types for transaction lifecycle:

```rust
#[derive(Serialize, Deserialize)]
enum TxEventType {
    ExecutionBegin,
    ExecutionComplete,
    // Future: Add more event types as needed
    // - ConsensusReceived
    // - ValidationBegin/Complete
    // - CertificateCreated
    // - etc.
}
```

## Logger Interface

### TransactionTraceLogger Struct
Shared logger accessed via `Arc<TransactionTraceLogger>`:

```rust
pub struct TransactionTraceLogger {
    config: TraceLogConfig,
    state: Mutex<LoggerState>,
    // Background task handle for flushing
}

struct LoggerState {
    /// Pre-allocated buffer for log records
    buffer: Vec<LogRecord>,
    /// Last event time for delta calculations (monotonic)
    last_instant: Instant,
    /// Time of last flush
    last_flush: Instant,
    /// Current file size tracker
    current_file_size: usize,
}

impl TransactionTraceLogger {
    /// Main logging interface - records a transaction event
    /// Logger handles time tracking and delta encoding internally
    pub fn write_transaction_event(
        &self,
        digest: TransactionDigest,
        event_type: TxEventType,
    ) -> Result<()>;
}
```

### Configuration
```rust
pub struct TraceLogConfig {
    /// Directory for log files
    pub log_dir: PathBuf,

    /// Maximum size per log file (default: 100MB)
    pub max_file_size: usize,

    /// Maximum number of log files to keep (default: 10)
    pub max_file_count: usize,

    /// Buffer capacity (default: 10,000 records)
    pub buffer_capacity: usize,

    /// Flush interval in seconds (default: 15)
    pub flush_interval_secs: u64,
}
```

### Buffering Strategy

**Goal**: Minimize critical section - no I/O, no allocation in the hot path.

**Double-buffering approach**:
1. Pre-allocate `Vec<LogRecord>` with large capacity (e.g., 10,000 records)
2. Critical section operations:
   ```rust
   lock mutex
   append LogRecord to buffer  // No allocation due to pre-allocated capacity
   check if flush needed:
     - buffer.len() == buffer.capacity() OR
     - elapsed time since last_flush > 15 seconds
   if flush needed:
     swap buffer with empty pre-allocated Vec
     send full buffer to background flush task
   unlock mutex
   ```
3. Background flush task:
   - Receives full buffer via channel
   - Serializes records with bincode
   - Writes to disk
   - Handles file rotation if needed
   - No mutex held during I/O

**Guarantees**:
- Critical section only does: append, time check, conditional swap
- No allocations in critical section (pre-allocated capacity)
- No I/O in critical section (delegated to background task)
- Minimal lock contention

### Operational Requirements

**Time Source**:
- Use `Instant::now()` for calculating deltas (monotonic, fast)
- Use `SystemTime::now()` for `AbsTime` records (wall-clock time for log analysis)

**Buffering**: Highly buffered writes - durability is not critical, minimize overhead is priority

**File Rotation**:
1. When current file reaches `max_file_size`:
   - Close current file
   - Open new file with timestamped name (e.g., `tx-trace-{unix_timestamp}.bin`)
   - Write `AbsTime(Instant::now())` as first record in new file
2. When file count exceeds `max_file_count`:
   - Delete oldest log file(s) to maintain limit

**Concurrency**: Logger must be safe for concurrent access from multiple threads logging interleaved transaction events

## Time Encoding Strategy

### Rationale
- Most transaction events occur in rapid succession (< 65ms apart)
- Use `DeltaTime(u16)` for common case: 2 bytes per time record
- Fall back to `DeltaTimeLarge(Duration)` when needed: ~12 bytes
- `AbsTime` anchors required at start of each log file

### Encoding Rules
1. First record in each log file: `AbsTime(SystemTime::now())`
2. Logger tracks last event time using `Instant` internally for delta calculations
3. For each subsequent time point:
   - Calculate microseconds since last time record using `Instant::elapsed()`
   - If Δt ≤ 65,535 µs: emit `DeltaTime(delta_micros as u16)`
   - If Δt > 65,535 µs: emit `DeltaTimeLarge(duration)`

### Example Log Sequence (Concurrent Transactions)
```
AbsTime(t0)                          // Anchor: t0
DeltaTime(100)
TransactionEvent(tx1, ExecutionBegin)
DeltaTime(50)
TransactionEvent(tx2, ExecutionBegin) // tx2 starts during tx1 execution
DeltaTime(200)
TransactionEvent(tx1, ExecutionComplete)
DeltaTime(100)
TransactionEvent(tx2, ExecutionComplete)
```

## Implementation Phases

### Phase 1: Core Types and Logging Infrastructure (COMPLETED)
- [x] Define `LogRecord` enum
- [x] Define `TxEventType` enum
- [x] Implement logger with file I/O
- [x] Time tracking and delta encoding logic
- [x] Basic tests
- [x] Double-buffering with background flush task
- [x] File rotation and cleanup logic
- [x] Synchronous flush mode for deterministic testing
- [x] Virtual time support with tokio test-util

### Phase 1.5: Log Reading and Replay (COMPLETED)
- [x] Define `TimestampedEvent` struct for replayed events
- [x] Implement `LogReader` for reading trace files
- [x] Reconstruct full timestamps from AbsTime + deltas
- [x] Iterator interface for event stream
- [x] Round-trip test verifying timestamp accuracy with virtual clock
- [x] Fixed virtual time support using tokio::time::Instant throughout

### Phase 2: Integration (CURRENT)
- [ ] Identify injection points in transaction execution pipeline
- [ ] Add instrumentation to execution flow
- [ ] Configuration and feature flags
- [ ] Performance benchmarks

### Phase 3: Tooling
- [ ] Log reader/parser utility
- [ ] Analysis tools (timeline visualization, statistics)
- [ ] Integration with monitoring systems

## File Format Specification

### Binary Layout
```
File: <sequence of length-prefixed BCS-serialized LogRecord>
Each record: [4-byte length (u32 LE)][BCS-encoded LogRecord]
```

Each `LogRecord` is serialized with BCS and prefixed with its length for sequential reading.

### Size Estimates (with 4-byte length prefix per record)
- `AbsTime`: ~4 (prefix) + ~24 bytes (BCS SystemTime encoding) = ~28 bytes
- `DeltaTime(u16)`: ~4 (prefix) + ~3 bytes (variant + u16) = ~7 bytes
- `DeltaTimeLarge(Duration)`: ~4 (prefix) + ~17 bytes (variant + Duration) = ~21 bytes
- `TransactionEvent`: ~4 (prefix) + ~34 bytes (variant + 32 bytes digest + event type) = ~38 bytes

Typical sequence (begin + complete):
```
AbsTime(28) + TxEvent(38) + DeltaTime(7) + TxEvent(38) = 111 bytes per transaction
```

With delta encoding, subsequent transactions only add ~83 bytes each.

## Design Decisions (RESOLVED)

1. **Time Source**: Dual approach ✓
   - `SystemTime` for `AbsTime` records (wall-clock time, serializable)
   - `Instant` for delta calculations (monotonic, fast)
2. **Buffering**: Double-buffer with pre-allocated capacity ✓
   - Pre-allocate Vec with large capacity (10K records default)
   - Swap buffers when full or after 15 seconds
   - Background task handles I/O
3. **Synchronization**: Single `Mutex` with minimal critical section ✓
   - No I/O in critical section
   - No allocation in critical section (pre-allocated buffers)
   - Only operations: append, time check, conditional swap
4. **File Rotation**: Size-based (100MB default) with count limit (10 default) ✓
5. **Concurrency**: Shared logger via `Arc<>`, handles concurrent writes ✓
6. **Interface**: Single method `write_transaction_event()` - logger handles time tracking ✓

## Open Questions

1. **Crate Location**: Where should this live?
   - Option A: New crate `sui-transaction-trace`
   - Option B: Add to existing `sui-core`
   - Option C: Add to `sui-types`

2. **Node Config Integration**:
   - Add `TraceLogConfig` to which config struct?
   - Enable/disable via feature flag or runtime config?

## Next Steps
1. Get feedback on design
2. Decide on crate location
3. Implement core types and logging infrastructure
4. Add basic tests
5. Benchmark serialization overhead
