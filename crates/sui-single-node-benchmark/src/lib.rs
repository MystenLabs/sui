// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::transaction::Transaction;
use tokio::sync::mpsc::Sender;
// use tracing::info;

use crate::benchmark_context::BenchmarkContext;
use crate::command::Component;
use crate::workload::Workload;

pub mod benchmark_context;
pub mod command;
pub(crate) mod mock_account;
pub(crate) mod mock_consensus;
pub(crate) mod mock_storage;
pub(crate) mod single_node;
pub(crate) mod tx_generator;
pub mod workload;

/// Benchmark a given workload on a specified component.
/// The different kinds of workloads and components can be found in command.rs.
/// \checkpoint_size represents both the size of a consensus commit, and size of a checkpoint
/// if we are benchmarking the checkpoint.
pub async fn run_benchmark(
    workload: Workload,
    component: Component,
    checkpoint_size: usize,
    out_channel: Option<Sender<Transaction>>,
) {
    println!("Setting up benchmark...");
    let start_time = std::time::Instant::now();
    let mut ctx = BenchmarkContext::new(workload, component, checkpoint_size).await;
    let tx_generator = workload.create_tx_generator(&mut ctx).await;
    let transactions = ctx.generate_transactions(tx_generator).await;
    let elapsed = start_time.elapsed().as_millis() as f64;
    println!(
        "Tx generation finished in {}ms at a rate of {} TPS",
        elapsed,
        1000f64 * workload.tx_count as f64 / elapsed,
    );
    match component {
        Component::TxnSigning => {
            ctx.benchmark_transaction_signing(transactions).await;
        }
        // Component::CheckpointExecutor => {
        //     ctx.benchmark_checkpoint_executor(transactions, checkpoint_size)
        //         .await;
        // }
        Component::ExecutionOnly => {
            ctx.benchmark_transaction_execution_in_memory(transactions)
                .await;
        }
        Component::PipeTxsToChannel => {
            ctx.benchmark_transaction_execution_with_channel(transactions, out_channel.unwrap())
                .await;
        }
        _ => {
            ctx.benchmark_transaction_execution(transactions).await;
        }
    }
}
