// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::benchmark_context::BenchmarkContext;
use crate::command::Component;
use crate::workload::Workload;
use sui_protocol_config::ProtocolConfig;

pub(crate) mod benchmark_context;
pub mod command;
pub(crate) mod mock_account;
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
    print_sample_tx: bool,
) {
    // This benchmark uses certify_transactions (QD path) which requires
    // disable_preconsensus_locking=false for signed transaction storage.
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_disable_preconsensus_locking_for_testing(false);
        config
    });

    let mut ctx = BenchmarkContext::new(workload.clone(), component, print_sample_tx).await;
    let tx_generator = workload.create_tx_generator(&mut ctx).await;
    let transactions = ctx.generate_transactions(tx_generator).await;

    // No consensus in single node benchmark so we do not need to go through
    // the certification process before assigning shared object versions.
    let assigned_versions = ctx
        .validator()
        .assigned_shared_object_versions(&transactions)
        .await;

    match component {
        Component::CheckpointExecutor => {
            ctx.benchmark_checkpoint_executor(transactions, assigned_versions, checkpoint_size)
                .await;
        }
        Component::ExecutionOnly => {
            ctx.benchmark_transaction_execution_in_memory(
                transactions,
                assigned_versions,
                print_sample_tx,
            )
            .await;
        }
        _ => {
            ctx.benchmark_transaction_execution(transactions, assigned_versions, print_sample_tx)
                .await;
        }
    }
}
