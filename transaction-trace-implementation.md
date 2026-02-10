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
    /// Last event time for delta calculations (monotonic, uses virtual time in tests)
    last_instant: tokio::time::Instant,
    /// Time of last flush
    last_flush: tokio::time::Instant,
    /// Initial time correlation for virtual time support
    initial_system_time: SystemTime,
    initial_instant: tokio::time::Instant,
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

    /// Use synchronous flushing (default: false, use async)
    /// Set to true in tests with current_thread runtime
    pub sync_flush: bool,
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
   - Receives full buffer (Vec<LogRecord>) via bounded channel (capacity: 1000)
   - Serializes entire batch with BCS using `bcs::serialized_size` and `bcs::serialize_into`
   - Writes single length prefix + batch to disk
   - Handles file rotation if needed
   - No mutex held during I/O
   - If channel is full, drops messages (no backpressure on hot path)

**Guarantees**:
- Critical section only does: append, time check, conditional swap
- No allocations in critical section (pre-allocated capacity)
- No I/O in critical section (delegated to background task)
- Minimal lock contention

### Operational Requirements

**Time Source**:
- Use `tokio::time::Instant::now()` for calculating deltas (monotonic, fast, respects virtual time in tests)
- Use computed `SystemTime` for `AbsTime` records (wall-clock time for log analysis)
- Virtual time correlation: track `initial_system_time` and `initial_instant` to compute SystemTime from elapsed virtual time

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
1. First record in each buffer: `AbsTime(computed from initial_system_time + elapsed virtual time)`
2. Logger tracks last event time using `tokio::time::Instant` internally for delta calculations
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
- [x] Integration test for file rotation and reconstruction

### Phase 2: Integration (CURRENT)
- [ ] Identify injection points in transaction execution pipeline
- [ ] Add instrumentation to execution flow
- [ ] Configuration and feature flags
- [ ] Performance benchmarks

### Phase 3: Tooling (IN PROGRESS)
- [x] **Chrome Trace Viewer converter** (`trace-to-chrome` binary)
  - Reads transaction trace logs and extracts digests
  - Fetches transaction data from Sui GraphQL endpoint
  - Maps each input object to a Chrome Trace "thread" (tid)
  - Visualizes object utilization and contention patterns
  - Supports fake data mode for testing
  - Generates JSON loadable in chrome://tracing
- [ ] Additional analysis tools (statistics, text reports)
- [ ] Integration with monitoring systems

## File Format Specification

### Binary Layout
```
File: <sequence of length-prefixed batches>
Each batch: [4-byte length (u32 LE)][BCS-encoded Vec<LogRecord>]
```

Records are written in batches (one batch per buffer flush). Each batch contains a Vec<LogRecord> serialized with BCS and prefixed with a single length for the entire batch. This minimizes overhead compared to per-record length prefixes.

### Size Estimates (batch format with single length prefix per flush)
Individual record sizes (within batch):
- `AbsTime`: ~24 bytes (BCS SystemTime encoding)
- `DeltaTime(u16)`: ~3 bytes (variant + u16)
- `DeltaTimeLarge(Duration)`: ~17 bytes (variant + Duration)
- `TransactionEvent`: ~34 bytes (variant + 32 bytes digest + event type)

Typical sequence per transaction (begin + complete):
```
AbsTime(24) + TxEvent(34) + DeltaTime(3) + TxEvent(34) = 95 bytes per transaction
```

With delta encoding, subsequent transactions only add ~71 bytes each. Plus ~4 bytes overhead for the entire batch (length prefix), and small BCS Vec overhead (ULEB128 length).

## Design Decisions (RESOLVED)

1. **Time Source**: Virtual time aware approach ✓
   - `tokio::time::Instant` for delta calculations (monotonic, respects virtual time)
   - Computed `SystemTime` from initial correlation + elapsed virtual time
   - Ensures consistent timestamps across buffer flushes in tests
2. **Serialization**: BCS (Binary Canonical Serialization) ✓
   - Actively maintained by Mysten Labs
   - Canonical/deterministic encoding
   - Length-prefixed records for sequential reading
3. **Buffering**: Double-buffer with pre-allocated capacity ✓
   - Pre-allocate Vec with large capacity (10K records default)
   - Swap buffers when full or after 15 seconds
   - Bounded channel (1000) with try_send (drops on full, no backpressure)
   - Background task handles I/O
4. **Synchronization**: Single `Mutex` with minimal critical section ✓
   - No I/O in critical section
   - No allocation in critical section (pre-allocated buffers)
   - Only operations: append, time check, conditional swap
5. **File Rotation**: Size-based (100MB default) with count limit (10 default) ✓
6. **Concurrency**: Shared logger via `Arc<>`, handles concurrent writes ✓
7. **Interface**: Single method `write_transaction_event()` - logger handles time tracking ✓
8. **Shutdown**: Drop impl flushes remaining buffered data ✓
9. **Testing**: Synchronous flush mode for deterministic tests with virtual time ✓

## Implementation Status

**Completed:**
- ✅ New crate `sui-transaction-trace` created
- ✅ Core types and logging infrastructure implemented
- ✅ Log reading and replay functionality with full timestamp reconstruction
- ✅ Comprehensive tests with virtual time support (deterministic, fast)
- ✅ BCS serialization with length-prefixed records
- ✅ Bounded channel with no-backpressure semantics
- ✅ Drop impl for shutdown flushing
- ✅ Clippy clean with all lints passing

**Remaining Work:**
1. **Node Integration**:
   - Identify injection points in transaction execution pipeline
   - Add instrumentation to execution flow
   - Add `TraceLogConfig` to node configuration
   - Feature flag or runtime config for enable/disable

2. **Performance**:
   - Benchmark serialization overhead
   - Measure impact on transaction throughput
   - Optimize if needed

3. **Tooling**:
   - Command-line log reader/parser utility
   - Analysis tools (timeline visualization, statistics)
   - Integration with monitoring systems
