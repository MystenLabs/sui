// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;
use sui_transaction_trace::*;

#[tokio::test]
async fn test_file_rotation_and_reconstruction() {
    telemetry_subscribers::init_for_testing();

    let temp_dir = tempfile::tempdir().unwrap();
    let config = TraceLogConfig {
        log_dir: temp_dir.path().to_path_buf(),
        max_file_size: 200, // Very small to force rotation (< 1 batch)
        max_file_count: 10,
        buffer_capacity: 10, // Flush every 10 records (5 transactions)
        flush_interval_secs: 100,
        sync_flush: false, // Use real async flush with background task
    };

    let logger = TransactionTraceLogger::new(config.clone()).unwrap();

    // Generate enough events to trigger multiple file rotations
    // Write in batches to force multiple flushes
    let mut expected_events = Vec::new();
    for batch in 0..4 {
        for i in 0..5 {
            let tx_id = batch * 5 + i;
            let digest = [tx_id as u8; 32];

            // Record begin event
            logger
                .write_transaction_event(digest, TxEventType::ExecutionBegin)
                .unwrap();
            expected_events.push((digest, TxEventType::ExecutionBegin));

            // Small delay between events (real time)
            tokio::time::sleep(Duration::from_micros(10)).await;

            // Record complete event
            logger
                .write_transaction_event(digest, TxEventType::ExecutionComplete)
                .unwrap();
            expected_events.push((digest, TxEventType::ExecutionComplete));

            // Small delay between transactions
            tokio::time::sleep(Duration::from_micros(10)).await;
        }
    }

    // Force final flush and wait for background task to complete
    drop(logger);
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify multiple log files were created
    let mut log_files: Vec<_> = std::fs::read_dir(temp_dir.path())
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

    assert!(
        log_files.len() >= 2,
        "Expected at least 2 log files due to rotation, got {}",
        log_files.len()
    );

    // Sort files by name (counter-based) to read in order
    log_files.sort_by_key(|e| e.path());

    // Read and reconstruct events from all files
    let mut reconstructed_events = Vec::new();
    for log_file in &log_files {
        let mut reader = LogReader::new(&log_file.path()).unwrap();
        for event in reader.iter() {
            let event = event.unwrap();
            reconstructed_events.push((event.digest, event.event_type, event.timestamp));
        }
    }

    // Verify we got all events
    assert_eq!(
        reconstructed_events.len(),
        expected_events.len(),
        "Expected {} events, got {}",
        expected_events.len(),
        reconstructed_events.len()
    );

    // Verify event order and content
    for (i, ((expected_digest, expected_type), (actual_digest, actual_type, _timestamp))) in
        expected_events
            .iter()
            .zip(reconstructed_events.iter())
            .enumerate()
    {
        assert_eq!(
            expected_digest, actual_digest,
            "Event {} digest mismatch",
            i
        );
        assert_eq!(expected_type, actual_type, "Event {} type mismatch", i);
    }

    // Verify timestamps are monotonically increasing across file boundaries
    for i in 1..reconstructed_events.len() {
        let (_, _, prev_ts) = reconstructed_events[i - 1];
        let (_, _, curr_ts) = reconstructed_events[i];
        assert!(
            curr_ts >= prev_ts,
            "Timestamp not monotonic at event {}: {:?} >= {:?}",
            i,
            curr_ts,
            prev_ts
        );
    }
}
