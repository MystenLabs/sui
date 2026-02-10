// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Transaction digest represented as a 32-byte array
pub type TransactionDigest = [u8; 32];

/// Log record types written to the trace file.
///
/// Records are serialized with bincode and written sequentially to binary log files.
/// Time records (AbsTime, DeltaTime, DeltaTimeLarge) anchor transaction events to
/// wall-clock time and record elapsed time between events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogRecord {
    /// Absolute timestamp anchor point (wall-clock time for log interpretation).
    /// Written at the start of each flushed buffer to correlate with wall-clock time.
    AbsTime(SystemTime),

    /// Delta time in microseconds (0-65535 µs = 0-65.5ms).
    /// Used for most time deltas as events are typically close together.
    DeltaTime(u16),

    /// Large delta for gaps > 65.5ms.
    /// Used when time between events exceeds u16::MAX microseconds.
    DeltaTimeLarge(Duration),

    /// Transaction event with digest and event type.
    /// Each event is self-contained with its own digest to support concurrent transactions.
    TransactionEvent {
        digest: TransactionDigest,
        event_type: TxEventType,
    },
}

/// Transaction event types for lifecycle tracking.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TxEventType {
    /// Transaction execution started
    ExecutionBegin,
    /// Transaction execution completed
    ExecutionComplete,
}

/// A transaction event with its reconstructed wall-clock timestamp.
///
/// This is the output type from LogReader - it provides a fully reconstructed
/// timestamp so consumers don't need to handle delta-time encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampedEvent {
    /// Wall-clock time when this event occurred
    pub timestamp: SystemTime,
    /// Transaction digest
    pub digest: TransactionDigest,
    /// Event type
    pub event_type: TxEventType,
}

/// Configuration for transaction trace logging.
///
/// Controls buffering, file rotation, and cleanup behavior.
#[derive(Debug, Clone)]
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

impl Default for TraceLogConfig {
    fn default() -> Self {
        Self {
            log_dir: PathBuf::from("transaction-traces"),
            max_file_size: 100 * 1024 * 1024, // 100MB
            max_file_count: 10,
            buffer_capacity: 10_000,
            flush_interval_secs: 15,
            sync_flush: false,
        }
    }
}

/// Internal logger state protected by mutex
struct LoggerState {
    /// Pre-allocated buffer for log records
    buffer: Vec<LogRecord>,
    /// Last event time for delta calculations (monotonic)
    last_instant: tokio::time::Instant,
    /// Time of last flush
    last_flush: tokio::time::Instant,
    /// Initial correlation between SystemTime and Instant for virtual time support
    initial_system_time: SystemTime,
    initial_instant: tokio::time::Instant,
}

/// State for the background flush task
struct FlushTaskState {
    /// Current log file being written to (buffered for performance)
    current_file: Option<BufWriter<std::fs::File>>,
    /// Size of the current file in bytes
    current_file_size: usize,
}

/// Transaction trace logger for recording execution timing.
///
/// This logger provides a low-overhead way to record transaction execution events
/// with precise timing. It uses:
/// - Double-buffering to minimize lock contention in the hot path
/// - Background flushing on a blocking thread for non-blocking writes (multi-threaded runtime)
/// - Synchronous flushing in single-threaded runtime (for testing)
/// - Delta-time encoding to keep log files compact
/// - Automatic file rotation and cleanup
///
/// # Thread Safety
/// The logger is thread-safe and designed for concurrent access from multiple threads.
/// The write path uses a mutex only for appending to the in-memory buffer, with no I/O
/// in the critical section (in multi-threaded mode).
///
/// # Example
/// ```no_run
/// use sui_transaction_trace::*;
///
/// let config = TraceLogConfig::default();
/// let logger = TransactionTraceLogger::new(config).unwrap();
///
/// let digest = [1u8; 32];
/// logger.write_transaction_event(digest, TxEventType::ExecutionBegin).unwrap();
/// logger.write_transaction_event(digest, TxEventType::ExecutionComplete).unwrap();
/// ```
pub struct TransactionTraceLogger {
    config: TraceLogConfig,
    state: Mutex<LoggerState>,
    /// Channel for async flushing (None in single-threaded runtime)
    flush_tx: Option<tokio::sync::mpsc::UnboundedSender<Vec<LogRecord>>>,
    /// Flush state for synchronous flushing in single-threaded mode
    sync_flush_state: Option<Mutex<FlushTaskState>>,
}

impl TransactionTraceLogger {
    /// Creates a new transaction trace logger.
    ///
    /// Detects the runtime type and uses either:
    /// - Async flushing with spawn_blocking (multi-threaded runtime)
    /// - Synchronous flushing (single-threaded runtime, for testing)
    ///
    /// # Errors
    /// Returns an error if the log directory cannot be created.
    pub fn new(config: TraceLogConfig) -> Result<Arc<Self>> {
        // Create log directory if it doesn't exist
        std::fs::create_dir_all(&config.log_dir)?;

        // Capture initial time correlation for virtual time support
        let initial_system_time = SystemTime::now();
        let initial_instant = tokio::time::Instant::now();

        let mut buffer = Vec::with_capacity(config.buffer_capacity);
        // Push initial AbsTime anchor
        buffer.push(LogRecord::AbsTime(initial_system_time));

        let (flush_tx, sync_flush_state) = if config.sync_flush {
            // Synchronous flushing mode (for tests)
            let flush_state = FlushTaskState {
                current_file: None,
                current_file_size: 0,
            };
            (None, Some(Mutex::new(flush_state)))
        } else {
            // Async flushing mode (production)
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let config_clone = config.clone();
            tokio::task::spawn_blocking(move || {
                Self::run_flush_task(config_clone, rx);
            });
            (Some(tx), None)
        };

        let logger = Arc::new(Self {
            config: config.clone(),
            state: Mutex::new(LoggerState {
                buffer,
                last_instant: initial_instant,
                last_flush: initial_instant,
                initial_system_time,
                initial_instant,
            }),
            flush_tx,
            sync_flush_state,
        });

        Ok(logger)
    }

    /// Computes current SystemTime based on virtual time.
    ///
    /// This ensures AbsTime records are consistent with delta times when using virtual time
    /// (e.g., in tests with tokio::time::pause()).
    fn current_system_time(state: &LoggerState) -> SystemTime {
        let elapsed = tokio::time::Instant::now() - state.initial_instant;
        state.initial_system_time + elapsed
    }

    /// Records a transaction event with automatic time tracking.
    ///
    /// This is the main logging interface. The logger automatically:
    /// - Calculates time delta since the last event
    /// - Adds appropriate time record (DeltaTime or DeltaTimeLarge)
    /// - Appends the transaction event
    /// - Triggers background flush if needed
    ///
    /// # Performance
    /// This method holds a mutex only for appending to the in-memory buffer.
    /// No I/O is performed in this call - all disk writes happen on a background thread.
    ///
    /// # Errors
    /// Returns an error only in exceptional cases (should not happen in normal operation).
    pub fn write_transaction_event(
        &self,
        digest: TransactionDigest,
        event_type: TxEventType,
    ) -> Result<()> {
        let now = tokio::time::Instant::now();

        let mut state = self.state.lock();

        // Check if we need to flush BEFORE adding records
        // Adding this event will add 2 records (time + event)
        let would_exceed_capacity = state.buffer.len() + 2 > self.config.buffer_capacity;
        let time_to_flush =
            now.duration_since(state.last_flush).as_secs() >= self.config.flush_interval_secs;
        let should_flush = (would_exceed_capacity || time_to_flush) && !state.buffer.is_empty();

        if should_flush {
            // Swap buffer with new empty one
            let mut new_buffer = Vec::with_capacity(self.config.buffer_capacity);
            std::mem::swap(&mut state.buffer, &mut new_buffer);
            state.last_flush = now;

            // Push AbsTime anchor to the new buffer and reset last_instant
            // This correlates the wall-clock time with the start of the new buffer
            // Use current_system_time to ensure consistency with virtual time
            let abs_time = Self::current_system_time(&state);
            state.buffer.push(LogRecord::AbsTime(abs_time));
            state.last_instant = tokio::time::Instant::now();

            // Flush buffer (async or sync depending on runtime)
            if let Some(flush_tx) = &self.flush_tx {
                // Async flush: send to background task
                drop(state);
                if let Err(e) = flush_tx.send(new_buffer) {
                    tracing::error!("Failed to send buffer to flush task, data lost: {:?}", e);
                }
                // Reacquire lock to add current event
                state = self.state.lock();
            } else if let Some(sync_flush_state) = &self.sync_flush_state {
                // Sync flush: flush immediately
                drop(state);
                let mut flush_state = sync_flush_state.lock();
                if let Err(e) =
                    Self::flush_buffer_to_disk(&self.config, &new_buffer, &mut flush_state)
                {
                    tracing::error!("Failed to flush transaction trace buffer: {}", e);
                }
                drop(flush_state);
                // Reacquire lock to add current event
                state = self.state.lock();
            }
        }

        // Calculate delta time since last record
        let elapsed = now.duration_since(state.last_instant);
        let micros = elapsed.as_micros();

        // Add time record
        if micros <= u16::MAX as u128 {
            state.buffer.push(LogRecord::DeltaTime(micros as u16));
        } else {
            state.buffer.push(LogRecord::DeltaTimeLarge(elapsed));
        }

        // Add transaction event
        state
            .buffer
            .push(LogRecord::TransactionEvent { digest, event_type });

        state.last_instant = now;

        Ok(())
    }

    /// Background task that flushes buffers to disk (runs on blocking thread)
    fn run_flush_task(
        config: TraceLogConfig,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<Vec<LogRecord>>,
    ) {
        let mut state = FlushTaskState {
            current_file: None,
            current_file_size: 0,
        };

        while let Some(buffer) = rx.blocking_recv() {
            if let Err(e) = Self::flush_buffer_to_disk(&config, &buffer, &mut state) {
                tracing::error!("Failed to flush transaction trace buffer: {}", e);
            }
        }

        tracing::info!("Transaction trace flush task exiting");
    }

    /// Flush a buffer to disk, handling file rotation
    fn flush_buffer_to_disk(
        config: &TraceLogConfig,
        buffer: &[LogRecord],
        state: &mut FlushTaskState,
    ) -> Result<()> {
        use std::io::Write;

        // Check if we need to rotate to a new file
        if state.current_file.is_none() || state.current_file_size >= config.max_file_size {
            // Explicitly flush and close current file before rotation
            if let Some(mut file) = state.current_file.take() {
                file.flush()?;
            }
            state.current_file_size = 0;

            // Create new file with buffered writer
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_secs();
            let file_path = config.log_dir.join(format!("tx-trace-{}.bin", timestamp));
            let file = std::fs::File::create(&file_path)?;
            let file = BufWriter::new(file);

            state.current_file = Some(file);

            // Clean up old files if needed
            Self::cleanup_old_files(config)?;
        }

        // Write all records in the buffer
        if let Some(file) = &mut state.current_file {
            for record in buffer {
                let encoded = bincode::serialize(record)?;
                file.write_all(&encoded)?;
                state.current_file_size += encoded.len();
            }
            file.flush()?;
        }

        Ok(())
    }

    /// Remove old log files if we exceed max_file_count
    fn cleanup_old_files(config: &TraceLogConfig) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(&config.log_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.starts_with("tx-trace-") && s.ends_with(".bin"))
                    .unwrap_or(false)
            })
            .collect();

        if entries.len() <= config.max_file_count {
            return Ok(());
        }

        // Sort by modification time
        entries.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));

        // Delete oldest files
        let to_delete = entries.len() - config.max_file_count;
        for entry in entries.iter().take(to_delete) {
            if let Err(e) = std::fs::remove_file(entry.path()) {
                tracing::warn!(
                    "Failed to delete old trace log file {:?}: {}",
                    entry.path(),
                    e
                );
            }
        }

        Ok(())
    }
}

impl Drop for TransactionTraceLogger {
    fn drop(&mut self) {
        // Flush any remaining buffered data
        let mut state = self.state.lock();
        if state.buffer.is_empty() {
            return;
        }

        let mut buffer_to_flush = Vec::new();
        std::mem::swap(&mut state.buffer, &mut buffer_to_flush);
        drop(state);

        if let Some(flush_tx) = &self.flush_tx {
            // Async mode: send to background task
            // Ignore errors - task might already be shut down
            let _ = flush_tx.send(buffer_to_flush);
        } else if let Some(sync_flush_state) = &self.sync_flush_state {
            // Sync mode: flush immediately
            let mut flush_state = sync_flush_state.lock();
            if let Err(e) =
                Self::flush_buffer_to_disk(&self.config, &buffer_to_flush, &mut flush_state)
            {
                tracing::error!("Failed to flush buffer on drop: {}", e);
            }
        }
    }
}

/// Reader for transaction trace log files.
///
/// Reads and parses binary log files, reconstructing full wall-clock timestamps
/// from AbsTime anchors and delta-time records. Provides an iterator interface
/// that yields TimestampedEvent structs.
///
/// # Example
/// ```no_run
/// use sui_transaction_trace::*;
/// use std::path::Path;
///
/// let reader = LogReader::new(Path::new("tx-trace-12345.bin")).unwrap();
/// for event in reader {
///     let event = event.unwrap();
///     println!("{:?} at {:?}", event.event_type, event.timestamp);
/// }
/// ```
pub struct LogReader {
    file: std::fs::File,
    current_time: Option<SystemTime>,
}

impl LogReader {
    /// Creates a new log reader for the specified file.
    ///
    /// # Errors
    /// Returns an error if the file cannot be opened.
    pub fn new(path: &std::path::Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        Ok(Self {
            file,
            current_time: None,
        })
    }

    /// Returns an iterator over timestamped events in the log file.
    pub fn iter(&mut self) -> LogReaderIterator<'_> {
        LogReaderIterator { reader: self }
    }
}

/// Iterator over timestamped events from a log file.
pub struct LogReaderIterator<'a> {
    reader: &'a mut LogReader,
}

impl<'a> Iterator for LogReaderIterator<'a> {
    type Item = Result<TimestampedEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Deserialize next record
            let record: LogRecord = match bincode::deserialize_from(&mut self.reader.file) {
                Ok(r) => r,
                Err(e) => {
                    // EOF is expected, other errors should be reported
                    if e.to_string().contains("unexpected end of file")
                        || e.to_string()
                            .contains("io error: failed to fill whole buffer")
                    {
                        return None;
                    }
                    return Some(Err(e.into()));
                }
            };

            match record {
                LogRecord::AbsTime(time) => {
                    // New absolute time anchor
                    self.reader.current_time = Some(time);
                }
                LogRecord::DeltaTime(micros) => {
                    // Add delta to current time
                    if let Some(current) = self.reader.current_time {
                        self.reader.current_time =
                            Some(current + Duration::from_micros(micros as u64));
                    } else {
                        return Some(Err(anyhow::anyhow!(
                            "DeltaTime record without preceding AbsTime"
                        )));
                    }
                }
                LogRecord::DeltaTimeLarge(duration) => {
                    // Add large delta to current time
                    if let Some(current) = self.reader.current_time {
                        self.reader.current_time = Some(current + duration);
                    } else {
                        return Some(Err(anyhow::anyhow!(
                            "DeltaTimeLarge record without preceding AbsTime"
                        )));
                    }
                }
                LogRecord::TransactionEvent { digest, event_type } => {
                    // Return event with reconstructed timestamp
                    if let Some(timestamp) = self.reader.current_time {
                        return Some(Ok(TimestampedEvent {
                            timestamp,
                            digest,
                            event_type,
                        }));
                    } else {
                        return Some(Err(anyhow::anyhow!(
                            "TransactionEvent without preceding AbsTime"
                        )));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn test_basic_logging() {
        tokio::time::pause();
        telemetry_subscribers::init_for_testing();

        let temp_dir = tempfile::tempdir().unwrap();
        let config = TraceLogConfig {
            log_dir: temp_dir.path().to_path_buf(),
            buffer_capacity: 4,       // Small capacity to trigger flush
            flush_interval_secs: 100, // Don't rely on time-based flush in tests
            sync_flush: true,
            ..Default::default()
        };

        let logger = TransactionTraceLogger::new(config).unwrap();

        // Log events to trigger capacity-based flush
        // Initial: [AbsTime] (1)
        // Event 0: check (1+2>4? no), add -> [AbsTime, Delta, Event] (3)
        // Event 1: check (3+2>4? yes), FLUSH
        let digest = [1u8; 32];
        logger
            .write_transaction_event(digest, TxEventType::ExecutionBegin)
            .unwrap();
        logger
            .write_transaction_event(digest, TxEventType::ExecutionComplete)
            .unwrap();

        // Check that a log file was created
        let entries: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(!entries.is_empty(), "Expected at least one log file");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_buffer_flush_on_capacity() {
        tokio::time::pause();
        telemetry_subscribers::init_for_testing();

        let temp_dir = tempfile::tempdir().unwrap();
        let config = TraceLogConfig {
            log_dir: temp_dir.path().to_path_buf(),
            buffer_capacity: 4,       // Small capacity to trigger flush quickly
            flush_interval_secs: 100, // Long interval so we test capacity-based flush
            sync_flush: true,
            ..Default::default()
        };

        let logger = TransactionTraceLogger::new(config).unwrap();

        // Log events to fill buffer
        // With capacity 4 and flush-before-add logic:
        // - Initial: [AbsTime] (1)
        // - Event 0: check (1+2>4? no), add -> [AbsTime, Delta, Event] (3)
        // - Event 1: check (3+2>4? yes), flush, add -> new [AbsTime, Delta, Event] (3)
        // - Event 2: check (3+2>4? yes), flush, add -> new [AbsTime, Delta, Event] (3)
        // - Event 3: check (3+2>4? yes), flush, add -> new [AbsTime, Delta, Event] (3)
        for i in 0..4 {
            let digest = [i as u8; 32];
            logger
                .write_transaction_event(digest, TxEventType::ExecutionBegin)
                .unwrap();
        }

        // Wait for async flush
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Check that a log file was created
        let entries: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(!entries.is_empty(), "Expected at least one log file");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_log_replay_round_trip() {
        tokio::time::pause();
        telemetry_subscribers::init_for_testing();

        let temp_dir = tempfile::tempdir().unwrap();
        let config = TraceLogConfig {
            log_dir: temp_dir.path().to_path_buf(),
            buffer_capacity: 10, // Small enough to trigger flush
            flush_interval_secs: 100,
            sync_flush: true,
            ..Default::default()
        };

        let logger = TransactionTraceLogger::new(config.clone()).unwrap();

        // Record events with known timing
        let tx1 = [1u8; 32];
        let tx2 = [2u8; 32];
        let tx3 = [3u8; 32];

        // Event 1: tx1 begins
        logger
            .write_transaction_event(tx1, TxEventType::ExecutionBegin)
            .unwrap();

        // Advance time by 100µs
        tokio::time::advance(Duration::from_micros(100)).await;

        // Event 2: tx2 begins (concurrent with tx1)
        logger
            .write_transaction_event(tx2, TxEventType::ExecutionBegin)
            .unwrap();

        // Advance time by 500µs
        tokio::time::advance(Duration::from_micros(500)).await;

        // Event 3: tx1 completes
        logger
            .write_transaction_event(tx1, TxEventType::ExecutionComplete)
            .unwrap();

        // Advance time by 200µs
        tokio::time::advance(Duration::from_micros(200)).await;

        // Event 4: tx2 completes
        logger
            .write_transaction_event(tx2, TxEventType::ExecutionComplete)
            .unwrap();

        // Advance time by 70ms (requires DeltaTimeLarge)
        tokio::time::advance(Duration::from_millis(70)).await;

        // Event 5: tx3 begins
        logger
            .write_transaction_event(tx3, TxEventType::ExecutionBegin)
            .unwrap();

        // Advance time by 100µs
        tokio::time::advance(Duration::from_micros(100)).await;

        // Event 6: tx3 completes
        logger
            .write_transaction_event(tx3, TxEventType::ExecutionComplete)
            .unwrap();

        // Force flush by filling buffer to capacity
        drop(logger);

        // Read back events
        let log_files: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.starts_with("tx-trace-") && s.ends_with(".bin"))
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(log_files.len(), 1, "Expected exactly one log file");

        let mut reader = LogReader::new(&log_files[0].path()).unwrap();
        let events: Vec<_> = reader.iter().collect::<Result<Vec<_>>>().unwrap();

        assert_eq!(events.len(), 6, "Expected 6 events");

        // Verify events match
        assert_eq!(events[0].digest, tx1);
        assert_eq!(events[0].event_type, TxEventType::ExecutionBegin);

        assert_eq!(events[1].digest, tx2);
        assert_eq!(events[1].event_type, TxEventType::ExecutionBegin);

        assert_eq!(events[2].digest, tx1);
        assert_eq!(events[2].event_type, TxEventType::ExecutionComplete);

        assert_eq!(events[3].digest, tx2);
        assert_eq!(events[3].event_type, TxEventType::ExecutionComplete);

        assert_eq!(events[4].digest, tx3);
        assert_eq!(events[4].event_type, TxEventType::ExecutionBegin);

        assert_eq!(events[5].digest, tx3);
        assert_eq!(events[5].event_type, TxEventType::ExecutionComplete);

        // Verify timestamps are reasonable (within tolerance due to encoding precision)
        // The timestamps should be monotonically increasing
        for i in 1..events.len() {
            assert!(
                events[i].timestamp >= events[i - 1].timestamp,
                "Timestamps should be monotonically increasing"
            );
        }

        // Check that time differences are approximately correct
        // Note: We can't check exact equality because:
        // 1. SystemTime::now() might advance slightly between events
        // 2. Delta encoding has microsecond precision

        // Time between event 0 and 1 should be ~100µs
        let delta_01 = events[1]
            .timestamp
            .duration_since(events[0].timestamp)
            .unwrap();
        assert!(
            delta_01 >= Duration::from_micros(90) && delta_01 <= Duration::from_micros(110),
            "Expected ~100µs between events 0 and 1, got {:?}",
            delta_01
        );

        // Time between event 1 and 2 should be ~500µs
        let delta_12 = events[2]
            .timestamp
            .duration_since(events[1].timestamp)
            .unwrap();
        assert!(
            delta_12 >= Duration::from_micros(490) && delta_12 <= Duration::from_micros(510),
            "Expected ~500µs between events 1 and 2, got {:?}",
            delta_12
        );

        // Time between event 3 and 4 should be ~200µs
        let delta_23 = events[3]
            .timestamp
            .duration_since(events[2].timestamp)
            .unwrap();
        assert!(
            delta_23 >= Duration::from_micros(190) && delta_23 <= Duration::from_micros(210),
            "Expected ~200µs between events 2 and 3, got {:?}",
            delta_23
        );

        // Time between event 4 and 5 should be ~70ms (tests DeltaTimeLarge)
        let delta_34 = events[4]
            .timestamp
            .duration_since(events[3].timestamp)
            .unwrap();
        assert!(
            delta_34 >= Duration::from_millis(69) && delta_34 <= Duration::from_millis(71),
            "Expected ~70ms between events 3 and 4, got {:?}",
            delta_34
        );

        // Time between event 5 and 6 should be ~100µs
        let delta_45 = events[5]
            .timestamp
            .duration_since(events[4].timestamp)
            .unwrap();
        assert!(
            delta_45 >= Duration::from_micros(90) && delta_45 <= Duration::from_micros(110),
            "Expected ~100µs between events 4 and 5, got {:?}",
            delta_45
        );
    }
}
