// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Generate a test trace file with real transaction digests for testing GraphQL fetching.

use anyhow::Result;
use std::time::Duration;
use sui_transaction_trace::*;

fn main() -> Result<()> {
    let temp_dir = std::path::PathBuf::from("test-traces-real");
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

    // Real transaction digests provided by user
    // 39Qhmds4WssMhxUYtPtU76aek8gbDuAVE1RdSqDyLp6M
    let tx1_b58 = "39Qhmds4WssMhxUYtPtU76aek8gbDuAVE1RdSqDyLp6M";
    // YCuWa3DdZoU9QZSVtXBzQkvKXe9GD5yDu9TQHM3wzkr
    let tx2_b58 = "YCuWa3DdZoU9QZSVtXBzQkvKXe9GD5yDu9TQHM3wzkr";

    // Decode base58 to get 32-byte digests
    let tx1_bytes = bs58::decode(tx1_b58)
        .into_vec()
        .expect("Failed to decode tx1");
    let tx2_bytes = bs58::decode(tx2_b58)
        .into_vec()
        .expect("Failed to decode tx2");

    assert_eq!(tx1_bytes.len(), 32, "Transaction digest must be 32 bytes");
    assert_eq!(tx2_bytes.len(), 32, "Transaction digest must be 32 bytes");

    let tx1: [u8; 32] = tx1_bytes.try_into().unwrap();
    let tx2: [u8; 32] = tx2_bytes.try_into().unwrap();

    println!("Generating test trace with real transaction digests...");
    println!("  TX1: {}", tx1_b58);
    println!("  TX2: {}", tx2_b58);

    // Transaction 1: starts at T+0ms, completes at T+100ms
    logger.write_transaction_event(tx1, TxEventType::ExecutionBegin)?;
    std::thread::sleep(Duration::from_millis(100));
    logger.write_transaction_event(tx1, TxEventType::ExecutionComplete)?;

    // Transaction 2: starts at T+150ms (after tx1 completes), completes at T+250ms
    std::thread::sleep(Duration::from_millis(50));
    logger.write_transaction_event(tx2, TxEventType::ExecutionBegin)?;
    std::thread::sleep(Duration::from_millis(100));
    logger.write_transaction_event(tx2, TxEventType::ExecutionComplete)?;

    // Force flush
    drop(logger);

    println!("\nGenerated test trace in {:?}", temp_dir);
    println!("Transaction 1: T+0ms to T+100ms");
    println!("Transaction 2: T+150ms to T+250ms");
    println!();
    println!("Run the converter with:");
    println!("  cargo run --bin trace_to_chrome -- -i test-traces-real -o trace-real.json");
    println!("Then open chrome://tracing and load trace-real.json");

    Ok(())
}
