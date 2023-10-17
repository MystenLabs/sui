// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::benchmark_context::BenchmarkContext;
use crate::command::Component;
use crate::workload::Workload;
use std::collections::BTreeMap;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::transaction::{CertifiedTransaction, Transaction};
use tracing::info;

/// Benchmark a given workload on a specified component.
/// The different kinds of workloads and components can be found in command.rs.
/// \checkpoint_size represents both the size of a consensus commit, and size of a checkpoint
/// if we are benchmarking the checkpoint.
pub async fn run_benchmark(workload: Workload, component: Component, checkpoint_size: usize) {
    let mut ctx = BenchmarkContext::new(workload, component, checkpoint_size).await;
    let tx_generator = workload.create_tx_generator(&mut ctx).await;
    let transactions = ctx.generate_transactions(tx_generator).await;
    benchmark_transactions(&ctx, transactions, component, checkpoint_size).await;
}

async fn benchmark_transactions(
    ctx: &BenchmarkContext,
    transactions: Vec<Transaction>,
    component: Component,
    checkpoint_size: usize,
) {
    match component {
        Component::TxnSigning => {
            benchmark_transaction_signing(ctx, transactions).await;
        }
        Component::CheckpointExecutor => {
            benchmark_checkpoint_executor(ctx, transactions, checkpoint_size).await;
        }
        _ => {
            benchmark_transaction_execution(ctx, transactions).await;
        }
    }
}

/// Benchmark parallel execution of a vector of transactions and measure the TPS.
async fn benchmark_transaction_execution(ctx: &BenchmarkContext, transactions: Vec<Transaction>) {
    let mut transactions = ctx.certify_transactions(transactions).await;
    execute_sample_transaction(ctx, transactions.pop().unwrap()).await;

    let tx_count = transactions.len();
    let start_time = std::time::Instant::now();
    info!(
        "Started executing {} transactions. You can now attach a profiler",
        transactions.len()
    );
    ctx.execute_transactions(transactions).await;
    let elapsed = start_time.elapsed().as_millis() as f64 / 1000f64;
    info!(
        "Execution finished in {}s, TPS={}",
        elapsed,
        tx_count as f64 / elapsed
    );
}

/// Print out a sample transaction and its effects so that we can get a rough idea
/// what we are measuring.
async fn execute_sample_transaction(
    ctx: &BenchmarkContext,
    sample_transaction: CertifiedTransaction,
) {
    info!("Sample transaction: {:?}", sample_transaction.data());
    let effects = ctx
        .validator()
        .execute_tx_immediately(sample_transaction.into_unsigned())
        .await;
    info!("Sample effects: {:?}\n\n", effects);
    assert!(effects.status().is_ok());
}

/// Benchmark parallel signing a vector of transactions and measure the TPS.
async fn benchmark_transaction_signing(ctx: &BenchmarkContext, transactions: Vec<Transaction>) {
    let sample_transaction = &transactions[0];
    info!("Sample transaction: {:?}", sample_transaction.data());

    let tx_count = transactions.len();
    let start_time = std::time::Instant::now();
    ctx.validator_sign_transactions(transactions).await;
    let elapsed = start_time.elapsed().as_millis() as f64 / 1000f64;
    info!(
        "Transaction signing finished in {}s, TPS={}.",
        elapsed,
        tx_count as f64 / elapsed,
    );
}

async fn benchmark_checkpoint_executor(
    ctx: &BenchmarkContext,
    transactions: Vec<Transaction>,
    checkpoint_size: usize,
) {
    let mut transactions = ctx.certify_transactions(transactions).await;

    execute_sample_transaction(ctx, transactions.pop().unwrap()).await;

    info!("Executing all transactions to generate effects");
    let tx_count = transactions.len();
    let effects: BTreeMap<_, _> = ctx
        .execute_transactions(transactions.clone())
        .await
        .into_iter()
        .map(|e| (*e.transaction_digest(), e))
        .collect();
    info!("Reverting all transactions so that we could re-execute through checkpoint executor");
    ctx.revert_transactions(effects.keys()).await;

    info!("Building checkpoints");
    let validator = ctx.validator();
    let checkpoints = validator
        .build_checkpoints(transactions, effects, checkpoint_size)
        .await;
    info!("Built {} checkpoints", checkpoints.len());
    let (mut checkpoint_executor, checkpoint_sender) = validator.create_checkpoint_executor();
    for (checkpoint, contents) in checkpoints {
        let state = validator.get_validator();
        state
            .get_checkpoint_store()
            .insert_verified_checkpoint(&checkpoint)
            .unwrap();
        state
            .database
            .multi_insert_transaction_and_effects(contents.iter())
            .unwrap();
        state
            .get_checkpoint_store()
            .insert_verified_checkpoint_contents(&checkpoint, contents)
            .unwrap();
        state
            .get_checkpoint_store()
            .update_highest_synced_checkpoint(&checkpoint)
            .unwrap();
        checkpoint_sender.send(checkpoint).unwrap();
    }
    let start_time = std::time::Instant::now();
    info!("Starting checkpoint execution. You can now attach a profiler");
    checkpoint_executor
        .run_epoch(validator.get_epoch_store().clone())
        .await;
    let elapsed = start_time.elapsed().as_millis() as f64 / 1000f64;
    info!(
        "Checkpoint execution finished in {}s, TPS={}.",
        elapsed,
        tx_count as f64 / elapsed,
    );
}
