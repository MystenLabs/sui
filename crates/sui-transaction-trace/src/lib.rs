// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// Transaction digest represented as a 32-byte array
pub type TransactionDigest = [u8; 32];

/// Log record types written to the trace file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogRecord {
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

/// Transaction event types
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TxEventType {
    ExecutionBegin,
    ExecutionComplete,
}

/// Configuration for transaction trace logging
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
}

impl Default for TraceLogConfig {
    fn default() -> Self {
        Self {
            log_dir: PathBuf::from("transaction-traces"),
            max_file_size: 100 * 1024 * 1024, // 100MB
            max_file_count: 10,
            buffer_capacity: 10_000,
            flush_interval_secs: 15,
        }
    }
}

/// Internal logger state protected by mutex
struct LoggerState {
    /// Pre-allocated buffer for log records
    buffer: Vec<LogRecord>,
    /// Last event time for delta calculations (monotonic)
    last_instant: Instant,
    /// Time of last flush
    last_flush: Instant,
}

/// State for the background flush task
struct FlushTaskState {
    /// Current log file being written to
    current_file: Option<std::fs::File>,
    /// Size of the current file in bytes
    current_file_size: usize,
}

/// Transaction trace logger
pub struct TransactionTraceLogger {
    config: TraceLogConfig,
    state: Mutex<LoggerState>,
    flush_tx: tokio::sync::mpsc::UnboundedSender<Vec<LogRecord>>,
}

impl TransactionTraceLogger {
    /// Create a new transaction trace logger
    pub fn new(config: TraceLogConfig) -> Result<Arc<Self>> {
        // Create log directory if it doesn't exist
        std::fs::create_dir_all(&config.log_dir)?;

        // Create channel for background flush task
        let (flush_tx, flush_rx) = tokio::sync::mpsc::unbounded_channel();

        let logger = Arc::new(Self {
            config: config.clone(),
            state: Mutex::new(LoggerState {
                buffer: Vec::with_capacity(config.buffer_capacity),
                last_instant: Instant::now(),
                last_flush: Instant::now(),
            }),
            flush_tx,
        });

        // Spawn background flush task on a dedicated thread
        // Uses std::thread::spawn for a long-lived thread doing blocking I/O
        let logger_clone = Arc::clone(&logger);
        std::thread::spawn(move || {
            logger_clone.run_flush_task(flush_rx);
        });

        Ok(logger)
    }

    /// Main logging interface - records a transaction event
    pub fn write_transaction_event(
        &self,
        digest: TransactionDigest,
        event_type: TxEventType,
    ) -> Result<()> {
        let now = Instant::now();

        let mut state = self.state.lock();

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

        // Check if we need to flush
        let should_flush = state.buffer.len() >= state.buffer.capacity()
            || now.duration_since(state.last_flush).as_secs() >= self.config.flush_interval_secs;

        if should_flush {
            // Swap buffer with new empty one
            let mut new_buffer = Vec::with_capacity(self.config.buffer_capacity);
            std::mem::swap(&mut state.buffer, &mut new_buffer);
            state.last_flush = now;

            // Send to flush task (drop lock before sending)
            drop(state);
            let _ = self.flush_tx.send(new_buffer);
        }

        Ok(())
    }

    /// Background task that flushes buffers to disk (runs on blocking thread)
    fn run_flush_task(&self, mut rx: tokio::sync::mpsc::UnboundedReceiver<Vec<LogRecord>>) {
        let mut state = FlushTaskState {
            current_file: None,
            current_file_size: 0,
        };

        while let Some(buffer) = rx.blocking_recv() {
            if let Err(e) = self.flush_buffer_to_disk(&buffer, &mut state) {
                tracing::error!("Failed to flush transaction trace buffer: {}", e);
            }
        }

        tracing::info!("Transaction trace flush task exiting");
    }

    /// Flush a buffer to disk, handling file rotation
    fn flush_buffer_to_disk(&self, buffer: &[LogRecord], state: &mut FlushTaskState) -> Result<()> {
        use std::io::Write;

        // Check if we need to rotate to a new file
        if state.current_file.is_none() || state.current_file_size >= self.config.max_file_size {
            // Close current file
            state.current_file = None;
            state.current_file_size = 0;

            // Create new file
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_secs();
            let file_path = self
                .config
                .log_dir
                .join(format!("tx-trace-{}.bin", timestamp));
            let mut file = std::fs::File::create(&file_path)?;

            // Write AbsTime as first record
            let abs_time_record = LogRecord::AbsTime(SystemTime::now());
            let encoded = bincode::serialize(&abs_time_record)?;
            file.write_all(&encoded)?;
            state.current_file_size += encoded.len();

            state.current_file = Some(file);

            // Clean up old files if needed
            self.cleanup_old_files()?;
        }

        // Write buffer to current file
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
    fn cleanup_old_files(&self) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(&self.config.log_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.starts_with("tx-trace-") && s.ends_with(".bin"))
                    .unwrap_or(false)
            })
            .collect();

        if entries.len() <= self.config.max_file_count {
            return Ok(());
        }

        // Sort by modification time
        entries.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));

        // Delete oldest files
        let to_delete = entries.len() - self.config.max_file_count;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_logging() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = TraceLogConfig {
            log_dir: temp_dir.path().to_path_buf(),
            buffer_capacity: 10,
            flush_interval_secs: 1,
            ..Default::default()
        };

        let logger = TransactionTraceLogger::new(config).unwrap();

        // Log some events
        let digest = [1u8; 32];
        logger
            .write_transaction_event(digest, TxEventType::ExecutionBegin)
            .unwrap();
        logger
            .write_transaction_event(digest, TxEventType::ExecutionComplete)
            .unwrap();

        // Wait for time-based flush (1 sec interval + buffer for processing)
        tokio::time::sleep(Duration::from_millis(1500)).await;

        // Trigger another write to check flush condition
        logger
            .write_transaction_event(digest, TxEventType::ExecutionBegin)
            .unwrap();

        // Wait for flush to complete
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Check that a log file was created
        let entries: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(!entries.is_empty(), "Expected at least one log file");
    }

    #[tokio::test]
    async fn test_buffer_flush_on_capacity() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = TraceLogConfig {
            log_dir: temp_dir.path().to_path_buf(),
            buffer_capacity: 4,       // Small capacity to trigger flush quickly
            flush_interval_secs: 100, // Long interval so we test capacity-based flush
            ..Default::default()
        };

        let logger = TransactionTraceLogger::new(config).unwrap();

        // Log events to fill buffer
        for i in 0..3 {
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
}
