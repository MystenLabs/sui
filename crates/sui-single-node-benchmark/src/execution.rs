// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::benchmark_context::BenchmarkContext;
use crate::command::Component;
use crate::workload::Workload;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::transaction::Transaction;
use tracing::info;

/// Benchmark a given workload on a specified component.
/// The different kinds of workloads and components can be found in command.rs.
pub async fn run_benchmark(workload: Workload, component: Component) {
    let mut ctx = BenchmarkContext::new(
        workload.num_accounts(),
        workload.gas_object_num_per_account(),
        component,
    )
    .await;
    let tx_generator = workload.create_tx_generator(&mut ctx).await;
    let transactions = ctx.generate_transactions(tx_generator).await;
    benchmark_transactions(&ctx, transactions, component).await;
}

async fn benchmark_transactions(
    ctx: &BenchmarkContext,
    transactions: Vec<Transaction>,
    component: Component,
) {
    match component {
        Component::TxnSigning => {
            benchmark_transaction_signing(ctx, transactions).await;
        }
        _ => {
            benchmark_transaction_execution(ctx, transactions).await;
        }
    }
}

/// Benchmark parallel execution of a vector of transactions and measure the TPS.
async fn benchmark_transaction_execution(ctx: &BenchmarkContext, transactions: Vec<Transaction>) {
    let mut transactions = ctx.certify_transactions(transactions).await;

    // Print out a sample transaction and its effects so that we can get a rough idea
    // what we are measuring.
    let sample_transaction = transactions.pop().unwrap();
    info!("Sample transaction: {:?}", sample_transaction.data());
    let effects = ctx
        .validator()
        .execute_tx_immediately(sample_transaction.into_unsigned())
        .await;
    info!("Sample effects: {:?}\n\n", effects);
    assert!(effects.status().is_ok());

    let tx_count = transactions.len();
    let start_time = std::time::Instant::now();
    ctx.execute_transactions(transactions).await;
    let elapsed = start_time.elapsed().as_millis() as f64 / 1000f64;
    info!(
        "Execution finished in {}s, TPS={}",
        elapsed,
        tx_count as f64 / elapsed
    );
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
