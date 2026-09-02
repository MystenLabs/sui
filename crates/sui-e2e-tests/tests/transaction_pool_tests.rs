// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_config::node::ConsensusTransactionPoolConfig;
use sui_core::authority_client::{
    AuthorityAPI, make_network_authority_clients_with_network_config,
};
use sui_macros::sim_test;
use sui_network::default_mysten_network_config;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::ObjectRef;
use sui_types::messages_grpc::{SubmitTxRequest, SubmitTxResult};
use sui_types::transaction::Transaction;
use test_cluster::TestClusterBuilder;

async fn make_transfer_tx_with_gas(
    cluster: &test_cluster::TestCluster,
    gas: ObjectRef,
    gas_price: u64,
) -> Transaction {
    let sender = cluster.get_address_0();
    let tx_data = TestTransactionBuilder::new(sender, gas, gas_price)
        .transfer_sui(Some(1), sender)
        .build();
    cluster.wallet.sign_transaction(&tx_data).await
}

async fn make_transfer_tx(cluster: &test_cluster::TestCluster, gas_price: u64) -> Transaction {
    let gas = cluster
        .wallet
        .get_one_gas_object_owned_by_address(cluster.get_address_0())
        .await
        .unwrap()
        .unwrap();
    make_transfer_tx_with_gas(cluster, gas, gas_price).await
}

fn pool_config(capacity: usize) -> ConsensusTransactionPoolConfig {
    ConsensusTransactionPoolConfig {
        max_pending_transactions: Some(capacity),
    }
}

// Only used by the `#[cfg(msim)]` fail-point tests below.
#[cfg(msim)]
fn gauge_value(
    families: Vec<prometheus::proto::MetricFamily>,
    name: &str,
    label: Option<(&str, &str)>,
) -> f64 {
    families
        .into_iter()
        .find(|family| family.name() == name)
        .and_then(|family| {
            family
                .get_metric()
                .iter()
                .find(|metric| {
                    label.is_none_or(|(label_name, label_value)| {
                        metric
                            .get_label()
                            .iter()
                            .any(|pair| pair.name() == label_name && pair.value() == label_value)
                    })
                })
                .cloned()
        })
        .map(|metric| metric.get_gauge().value())
        .unwrap_or_default()
}

#[sim_test]
async fn test_transaction_pool_basic_flow_and_epoch_progress() {
    let network_config = ConfigBuilder::new_with_temp_dir()
        .with_consensus_transaction_pool_config(pool_config(100))
        .build();
    let committee = network_config.committee_with_network();
    let cluster = TestClusterBuilder::new()
        .set_network_config(network_config)
        .build()
        .await;
    let gas_price = cluster.get_reference_gas_price().await;

    let transaction = make_transfer_tx(&cluster, gas_price).await;
    let result = cluster
        .wallet
        .execute_transaction_may_fail(transaction)
        .await;
    assert!(result.is_ok(), "pull-mode transaction should succeed");

    // Pings take the other submission path in pull mode: RPC -> ConsensusAdapter ->
    // TransactionPoolClient -> the pool's ping lane, acked by the real proposer.
    let clients = make_network_authority_clients_with_network_config(
        &committee,
        &default_mysten_network_config(),
    );
    let validator = cluster.all_validator_handles().into_iter().next().unwrap();
    let validator_name = validator.with(|node| node.state().name);
    let response = clients
        .get(&validator_name)
        .unwrap()
        .submit_transaction(SubmitTxRequest::new_ping(), None)
        .await
        .expect("ping should resolve through the pool's ping lane");
    assert!(matches!(
        response.results[0],
        SubmitTxResult::Submitted { .. }
    ));

    cluster.trigger_reconfiguration().await;
    let transaction = make_transfer_tx(&cluster, gas_price).await;
    let result = cluster
        .wallet
        .execute_transaction_may_fail(transaction)
        .await;
    assert!(
        result.is_ok(),
        "system traffic and user submissions should progress after reconfiguration"
    );
}

// Depends on per-simnode fail-point gating, so it only compiles under msim.
#[cfg(msim)]
#[sim_test]
async fn test_transaction_pool_eviction_and_rejection() {
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use sui_macros::{clear_fail_point, register_fail_point_if};

    let take_disabled = Arc::new(AtomicBool::new(true));
    let network_config = ConfigBuilder::new_with_temp_dir()
        .committee_size(NonZeroUsize::new(4).unwrap())
        .with_consensus_transaction_pool_config(pool_config(2))
        .build();
    let committee = network_config.committee_with_network();
    let cluster = TestClusterBuilder::new()
        .set_network_config(network_config)
        .build()
        .await;
    let validator = cluster.all_validator_handles().into_iter().next().unwrap();
    let target_node = validator.with(|_| sui_simulator::current_simnode_id());
    let disabled = take_disabled.clone();
    validator.with(|_| {
        register_fail_point_if("consensus_transaction_pool_disable_take", move || {
            sui_simulator::current_simnode_id() == target_node && disabled.load(Ordering::SeqCst)
        });
    });

    let clients = make_network_authority_clients_with_network_config(
        &committee,
        &default_mysten_network_config(),
    );
    let validator_name = validator.with(|node| node.state().name);
    let client = clients.get(&validator_name).unwrap();
    let gas_price = cluster.get_reference_gas_price().await;
    let sender = cluster.get_address_0();
    let gas_objects = cluster
        .wallet
        .get_all_gas_objects_owned_by_address(sender)
        .await
        .unwrap();
    let first = make_transfer_tx_with_gas(&cluster, gas_objects[0], gas_price).await;
    let second = make_transfer_tx_with_gas(&cluster, gas_objects[1], gas_price).await;
    let low = make_transfer_tx_with_gas(&cluster, gas_objects[2], gas_price).await;
    let high = make_transfer_tx_with_gas(&cluster, gas_objects[3], gas_price * 10).await;

    let first_task = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .submit_transaction(SubmitTxRequest::new_transaction(first), None)
                .await
        }
    });
    let second_task = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .submit_transaction(SubmitTxRequest::new_transaction(second), None)
                .await
        }
    });
    loop {
        let depth = validator.with(|node| {
            gauge_value(
                node.prometheus_metrics_for_testing(),
                "consensus_transaction_pool_depth",
                Some(("lane", "user")),
            )
        });
        if depth == 2.0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let error = client
        .submit_transaction(SubmitTxRequest::new_transaction(low), None)
        .await
        .expect_err("a low-price newcomer must be rejected when the pool is full");
    assert!(error.to_string().contains("outbid"));

    let high_task = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .submit_transaction(SubmitTxRequest::new_transaction(high), None)
                .await
        }
    });
    while !first_task.is_finished() && !second_task.is_finished() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    take_disabled.store(false, Ordering::SeqCst);
    validator.with(|_| {
        clear_fail_point("consensus_transaction_pool_disable_take");
    });

    let first_result = first_task.await.unwrap();
    let second_result = second_task.await.unwrap();
    let high_result = high_task.await.unwrap();
    assert!(high_result.is_ok());
    assert_eq!(
        [first_result, second_result]
            .iter()
            .filter(|result| result.is_ok())
            .count(),
        1
    );
}

// Depends on per-simnode fail-point gating, so it only compiles under msim.
#[cfg(msim)]
#[sim_test]
async fn test_transaction_pool_epoch_boundary_resolves_pending_submission() {
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use sui_macros::{clear_fail_point, register_fail_point_if};

    let take_disabled = Arc::new(AtomicBool::new(true));
    let network_config = ConfigBuilder::new_with_temp_dir()
        .committee_size(NonZeroUsize::new(4).unwrap())
        .with_consensus_transaction_pool_config(pool_config(10))
        .build();
    let committee = network_config.committee_with_network();
    let cluster = TestClusterBuilder::new()
        .set_network_config(network_config)
        .build()
        .await;
    let validator = cluster.all_validator_handles().into_iter().next().unwrap();
    let start_epoch = validator.with(|node| node.state().epoch_store_for_testing().epoch());
    let target_node = validator.with(|_| sui_simulator::current_simnode_id());
    let disabled = take_disabled.clone();
    validator.with(|_| {
        register_fail_point_if("consensus_transaction_pool_disable_user_take", move || {
            sui_simulator::current_simnode_id() == target_node && disabled.load(Ordering::SeqCst)
        });
    });

    let clients = make_network_authority_clients_with_network_config(
        &committee,
        &default_mysten_network_config(),
    );
    let validator_name = validator.with(|node| node.state().name);
    let client = clients.get(&validator_name).unwrap().clone();
    let gas_price = cluster.get_reference_gas_price().await;
    let transaction = make_transfer_tx(&cluster, gas_price).await;
    let submission = tokio::spawn(async move {
        client
            .submit_transaction(SubmitTxRequest::new_transaction(transaction), None)
            .await
    });
    loop {
        let depth = validator.with(|node| {
            gauge_value(
                node.prometheus_metrics_for_testing(),
                "consensus_transaction_pool_depth",
                Some(("lane", "user")),
            )
        });
        if depth == 1.0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    cluster.trigger_reconfiguration().await;
    take_disabled.store(false, Ordering::SeqCst);
    let response = submission.await.unwrap();
    assert!(
        response.is_ok(),
        "pending submission should retry into the new epoch: {response:?}"
    );
    let response = response.unwrap();
    let SubmitTxResult::Submitted { consensus_position } = &response.results[0] else {
        panic!("expected a submitted consensus position");
    };
    assert_eq!(consensus_position.epoch, start_epoch + 1);
    validator.with(|_| {
        clear_fail_point("consensus_transaction_pool_disable_user_take");
    });
}

// Depends on per-simnode fail-point gating, so it only compiles under msim.
#[cfg(msim)]
#[sim_test]
async fn test_transaction_pool_epoch_store_swap_waits_for_new_pool() {
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use sui_macros::{clear_fail_point, register_fail_point_async};
    use tokio::sync::broadcast;

    let network_config = ConfigBuilder::new_with_temp_dir()
        .committee_size(NonZeroUsize::new(4).unwrap())
        .with_consensus_transaction_pool_config(pool_config(10))
        .build();
    let committee = network_config.committee_with_network();
    let cluster = TestClusterBuilder::new()
        .set_network_config(network_config)
        .build()
        .await;
    let validator = cluster.all_validator_handles().into_iter().next().unwrap();
    let start_epoch = validator.with(|node| node.state().epoch_store_for_testing().epoch());
    let target_node = validator.with(|_| sui_simulator::current_simnode_id());
    let (release, _) = broadcast::channel::<()>(1);
    let reached = Arc::new(AtomicBool::new(false));
    let reached_callback = reached.clone();
    let release_callback = release.clone();
    validator.with(|_| {
        register_fail_point_async(
            "consensus_transaction_pool_reconfig_before_shutdown",
            move || {
                let current_node = sui_simulator::current_simnode_id();
                let reached_callback = reached_callback.clone();
                let mut receiver = release_callback.subscribe();
                async move {
                    if current_node == target_node {
                        reached_callback.store(true, Ordering::SeqCst);
                        receiver.recv().await.unwrap();
                    }
                }
            },
        );
    });

    let clients = make_network_authority_clients_with_network_config(
        &committee,
        &default_mysten_network_config(),
    );
    let validator_name = validator.with(|node| node.state().name);
    let client = clients.get(&validator_name).unwrap().clone();
    let gas_price = cluster.get_reference_gas_price().await;
    let transaction = make_transfer_tx(&cluster, gas_price).await;

    let reconfiguration = cluster.trigger_reconfiguration();
    tokio::pin!(reconfiguration);
    let mut reconfig_done = false;
    while !reached.load(Ordering::SeqCst) {
        if reconfig_done {
            tokio::time::sleep(Duration::from_millis(10)).await;
            continue;
        }
        tokio::select! {
            _ = &mut reconfiguration => {
                reconfig_done = true;
            }
            _ = tokio::time::sleep(Duration::from_millis(10)) => {}
        }
    }

    let waiting_before = validator.with(|node| {
        gauge_value(
            node.prometheus_metrics_for_testing(),
            "consensus_transaction_pool_waiting_inserts",
            None,
        )
    });
    let submission = tokio::spawn(async move {
        client
            .submit_transaction(SubmitTxRequest::new_transaction(transaction), None)
            .await
    });
    loop {
        let waiting = validator.with(|node| {
            gauge_value(
                node.prometheus_metrics_for_testing(),
                "consensus_transaction_pool_waiting_inserts",
                None,
            )
        });
        if waiting > waiting_before {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    release.send(()).unwrap();
    validator.with(|_| {
        clear_fail_point("consensus_transaction_pool_reconfig_before_shutdown");
    });
    if !reconfig_done {
        reconfiguration.await;
    }
    let result = submission.await.unwrap();
    assert!(
        result.is_ok(),
        "submission should retry through the matching-epoch pool: {result:?}"
    );
    let response = result.unwrap();
    let SubmitTxResult::Submitted { consensus_position } = &response.results[0] else {
        panic!("expected a submitted consensus position");
    };
    assert_eq!(consensus_position.epoch, start_epoch + 1);

    cluster.trigger_reconfiguration().await;
}
