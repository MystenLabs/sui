// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;
use sui_macros::sim_test;
use sui_test_transaction_builder::publish_basics_package_and_make_counter;
use sui_types::base_types::dbg_addr;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn test_validator_tx_finalizer_fastpath_tx() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(7)
        // Make epoch duration large enough so that reconfig is never triggered.
        .with_epoch_duration_ms(1000 * 1000)
        .build()
        .await;
    let tx_data = cluster
        .test_transaction_builder()
        .await
        .transfer_sui(None, dbg_addr(1))
        .build();
    let tx = cluster.sign_transaction(&tx_data);
    let tx_digest = *tx.digest();
    // Only broadcast to get a certificate, but do not execute it.
    cluster
        .authority_aggregator()
        .process_transaction(tx, None)
        .await
        .unwrap();
    // Since 2f+1 signed the tx, i.e. 5 validators have signed the tx, in the worst case where the other 2 wake up first,
    // it would take 10 + 3 * 1 = 13s for a validator to finalize this.
    let tx_digests = [tx_digest];
    tokio::time::timeout(Duration::from_secs(60), async move {
        for node in cluster.all_node_handles() {
            node.with_async(|n| async {
                n.state()
                    .get_transaction_cache_reader()
                    .notify_read_executed_effects_digests(&tx_digests)
                    .await;
            })
            .await;
        }
    })
    .await
    .unwrap();
}

#[sim_test]
async fn test_validator_tx_finalizer_consensus_tx() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(7)
        // Make epoch duration large enough so that reconfig is never triggered.
        .with_epoch_duration_ms(1000 * 1000)
        .build()
        .await;
    let (package, counter) = publish_basics_package_and_make_counter(&cluster.wallet).await;
    let tx_data = cluster
        .test_transaction_builder()
        .await
        .call_counter_increment(package.0, counter.0, counter.1)
        .build();
    let tx = cluster.sign_transaction(&tx_data);
    let tx_digest = *tx.digest();
    // Only broadcast to get a certificate, but do not execute it.
    cluster
        .authority_aggregator()
        .process_transaction(tx, None)
        .await
        .unwrap();
    let tx_digests = [tx_digest];
    tokio::time::timeout(Duration::from_secs(60), async move {
        for node in cluster.all_node_handles() {
            node.with_async(|n| async {
                n.state()
                    .get_transaction_cache_reader()
                    .notify_read_executed_effects_digests(&tx_digests)
                    .await;
            })
            .await;
        }
    })
    .await
    .unwrap();
}

#[cfg(msim)]
#[sim_test]
async fn test_validator_tx_finalizer_equivocation() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(7)
        // Make epoch duration large enough so that reconfig is never triggered.
        .with_epoch_duration_ms(1000 * 1000)
        .build()
        .await;
    let tx_data1 = cluster
        .test_transaction_builder()
        .await
        .transfer_sui(None, dbg_addr(1))
        .build();
    let tx1 = cluster.sign_transaction(&tx_data1);
    let tx_data2 = cluster
        .test_transaction_builder()
        .await
        .transfer_sui(None, dbg_addr(2))
        .build();
    let tx2 = cluster.sign_transaction(&tx_data2);
    let tx_digest1 = *tx1.digest();
    let tx_digest2 = *tx2.digest();
    let auth_agg = cluster.authority_aggregator();
    for (idx, client) in auth_agg.authority_clients.values().enumerate() {
        if idx % 2 == 0 {
            client.handle_transaction(tx1.clone(), None).await.unwrap();
        } else {
            client.handle_transaction(tx2.clone(), None).await.unwrap();
        }
    }
    // It takes up to 11s (5 + 6 * 1) for each validator to wake up and finalize the txs once.
    // We wait for long enough and check that no validator will spawn a thread
    // twice to try to finalize the same txs.
    tokio::time::sleep(Duration::from_secs(30)).await;
    for node in cluster.swarm.validator_node_handles() {
        node.with(|n| {
            let state = n.state();
            assert!(!state.is_tx_already_executed(&tx_digest1));
            assert!(!state.is_tx_already_executed(&tx_digest2));
            assert_eq!(
                state
                    .validator_tx_finalizer
                    .as_ref()
                    .unwrap()
                    .num_finalization_attempts_for_testing(),
                1
            );
        });
    }
}
