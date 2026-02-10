// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Generate a test trace file with fake timing data for testing the Chrome Trace converter.

use anyhow::Result;
use std::time::{Duration, SystemTime};
use sui_transaction_trace::*;

fn main() -> Result<()> {
    let temp_dir = std::path::PathBuf::from("test-traces");
    std::fs::create_dir_all(&temp_dir)?;

    let config = TraceLogConfig {
        log_dir: temp_dir.clone(),
        max_file_size: 100 * 1024 * 1024,
        max_file_count: 10,
        buffer_capacity: 1000,
        flush_interval_secs: 60,
        sync_flush: true,
    };

    let logger = TransactionTraceLogger::new(config)?;

    // Create fake transaction digests (32 bytes each)
    // In a real scenario, these would be actual transaction digests
    let tx1_bytes = [1u8; 32]; // Represents 39Qhmds4WssMhxUYtPtU76aek8gbDuAVE1RdSqDyLp6M
    let tx2_bytes = [2u8; 32]; // Represents YCuWa3DdZoU9QZSVtXBzQkvKXe9GD5yDu9TQHM3wzkr

    println!("Generating test trace with fake timing data...");

    // Transaction 1: starts at T+0ms, completes at T+100ms
    logger.write_transaction_event(tx1_bytes, TxEventType::ExecutionBegin)?;
    std::thread::sleep(Duration::from_millis(100));
    logger.write_transaction_event(tx1_bytes, TxEventType::ExecutionComplete)?;

    // Transaction 2: starts at T+150ms (after tx1 completes), completes at T+250ms
    std::thread::sleep(Duration::from_millis(50));
    logger.write_transaction_event(tx2_bytes, TxEventType::ExecutionBegin)?;
    std::thread::sleep(Duration::from_millis(100));
    logger.write_transaction_event(tx2_bytes, TxEventType::ExecutionComplete)?;

    // Force flush
    drop(logger);

    println!("Generated test trace in {:?}", temp_dir);
    println!("Transaction 1 (39Qhmds...): T+0ms to T+100ms");
    println!("Transaction 2 (YCuWa3D...): T+150ms to T+250ms");
    println!();
    println!("Run the converter with:");
    println!("  cargo run --bin trace-to-chrome -- -i test-traces -o trace.json --fake-data");
    println!("Then open chrome://tracing and load trace.json");

    Ok(())
}
