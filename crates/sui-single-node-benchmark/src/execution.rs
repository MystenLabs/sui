// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::benchmark_context::BenchmarkContext;
use crate::command::Component;
use crate::tx_generator::{MoveTxGenerator, NonMoveTxGenerator, RootObjectCreateTxGenerator};
use std::collections::HashMap;
use std::sync::Arc;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::transaction::Transaction;
use tracing::info;

/// Benchmark simple transfer transactions.
/// Each transaction transfers the gas object from an address to itself.
/// The execution does not invoke Move VM, and is considered the cheapest kind of transaction.
///
/// \tx_count: the number of transactions to execute.
/// \component: The component to benchmark.
pub async fn benchmark_simple_transfer(tx_count: u64, component: Component) {
    let ctx = BenchmarkContext::new(tx_count, 1, component).await;
    let transactions = ctx
        .generate_transactions(Arc::new(NonMoveTxGenerator::new()))
        .await;
    benchmark_transactions(&ctx, transactions, component).await;
}

/// Benchmark Move transactions.
/// Each transaction is a programmable transaction that performs a series of operations specified
/// by the parameters.
///
/// \tx_count: the number of transactions to execute.
/// \component: The component to benchmark.
/// \num_input_objects: the number of address owned input coin objects for each transaction.
/// These objects will be read during input checking, and merged during execution.
/// \num_dynamic_fields: the number of dynamic fields read during execution of each transaction.
/// \computation: Computation intensity for each transaction.
pub async fn benchmark_move_transactions(
    tx_count: u64,
    component: Component,
    num_input_objects: u8,
    num_dynamic_fields: u64,
    computation: u8,
) {
    assert!(
        num_input_objects >= 1,
        "Each transaction requires at least 1 input object"
    );
    let mut ctx = BenchmarkContext::new(tx_count, num_input_objects as u64, component).await;
    let move_package = ctx.publish_package().await;
    let root_objects = preparing_dynamic_fields(&mut ctx, move_package.0, num_dynamic_fields).await;
    let transactions = ctx
        .generate_transactions(Arc::new(MoveTxGenerator::new(
            move_package.0,
            num_input_objects,
            computation,
            root_objects,
        )))
        .await;
    benchmark_transactions(&ctx, transactions, component).await;
}

/// In order to benchmark transactions that can read dynamic fields, we must first create
/// a root object with dynamic fields for each account address.
async fn preparing_dynamic_fields(
    ctx: &mut BenchmarkContext,
    move_package: ObjectID,
    num_dynamic_fields: u64,
) -> HashMap<SuiAddress, ObjectRef> {
    if num_dynamic_fields == 0 {
        return HashMap::new();
    }

    info!("Preparing root object with dynamic fields");
    let root_object_create_transactions = ctx
        .generate_transactions(Arc::new(RootObjectCreateTxGenerator::new(
            move_package,
            num_dynamic_fields,
        )))
        .await;
    let results = ctx
        .execute_transactions_immediately(root_object_create_transactions)
        .await;
    let mut root_objects = HashMap::new();
    let mut new_gas_objects = HashMap::new();
    for effects in results {
        let (owner, root_object) = effects
            .created()
            .into_iter()
            .filter_map(|(oref, owner)| {
                owner
                    .get_address_owner_address()
                    .ok()
                    .map(|owner| (owner, oref))
            })
            .next()
            .unwrap();
        let gas_object = effects.gas_object().0;
        root_objects.insert(owner, root_object);
        new_gas_objects.insert(gas_object.0, gas_object);
    }
    ctx.refresh_gas_objects(new_gas_objects);
    info!("Finished preparing root object with dynamic fields");
    root_objects
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
