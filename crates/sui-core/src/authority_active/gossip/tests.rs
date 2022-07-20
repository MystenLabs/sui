// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority_active::gossip::configurable_batch_action_client::{
    init_configurable_authorities, BatchAction, ConfigurableBatchActionClient,
};
use crate::authority_active::MAX_RETRY_DELAY_MS;
use std::time::Duration;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "current_thread", start_paused = true)]
pub async fn test_gossip_plain() {
    let action_sequence = vec![
        BatchAction::EmitUpdateItem(),
        BatchAction::EmitUpdateItem(),
        BatchAction::EmitUpdateItem(),
    ];

    let (net, states, digests) = init_configurable_authorities(action_sequence).await;

    let _active_authorities = start_gossip_process(states.clone(), net.clone()).await;
    tokio::time::sleep(Duration::from_secs(20)).await;

    // Expected outcome of gossip: each digest's tx signature and cert is now on every authority.
    for client in net.clone_inner_clients().values() {
        for digest in &digests {
            let result1 = client
                .handle_transaction_info_request(TransactionInfoRequest {
                    transaction_digest: digest.transaction,
                })
                .await;

            assert!(result1.is_ok());
            let result = result1.unwrap();
            let found_cert = result.certified_transaction.is_some();
            assert!(found_cert);
        }
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
pub async fn test_gossip_error() {
    let action_sequence = vec![BatchAction::EmitError(), BatchAction::EmitUpdateItem()];

    let (net, states, digests) = init_configurable_authorities(action_sequence).await;

    let _active_authorities = start_gossip_process(states.clone(), net.clone()).await;
    // failure back-offs were set from the errors
    tokio::time::sleep(Duration::from_millis(MAX_RETRY_DELAY_MS)).await;

    for client in net.clone_inner_clients().values() {
        for digest in &digests {
            let result1 = client
                .handle_transaction_info_request(TransactionInfoRequest {
                    transaction_digest: digest.transaction,
                })
                .await;

            assert!(result1.is_ok());
            let result = result1.unwrap();
            let found_cert = result.certified_transaction.is_some();
            assert!(found_cert);
        }
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
pub async fn test_gossip_after_revert() {
    let action_sequence = vec![BatchAction::EmitUpdateItem(), BatchAction::EmitUpdateItem()];
    let (net, states, digests) = init_configurable_authorities(action_sequence).await;

    tokio::time::sleep(Duration::from_secs(20)).await;
    // 3 (quorum) of the validators have executed 2 transactions, and 1 has none.
    let all_seq = states
        .iter()
        .map(|s| s.database.next_sequence_number().unwrap());
    assert_eq!(all_seq.clone().filter(|s| s == &2).count(), 3,);
    assert_eq!(all_seq.filter(|s| s == &0).count(), 1,);

    // There are 2 transactions:
    // 1. For the first transaction, only one validator reverts it.
    // 2. For the second transaction, all validators revert it.
    for state in &states {
        if state.get_transaction(digests[0].transaction).await.is_ok() {
            state
                .database
                .revert_state_update(&digests[0].transaction)
                .unwrap();
            break;
        }
    }
    for state in &states {
        if state.get_transaction(digests[1].transaction).await.is_ok() {
            state
                .database
                .revert_state_update(&digests[1].transaction)
                .unwrap();
        }
    }

    let _active_authorities = start_gossip_process(states.clone(), net.clone()).await;
    tokio::time::sleep(Duration::from_secs(20)).await;

    for client in net.clone_inner_clients().values() {
        let result = client
            .handle_transaction_info_request(TransactionInfoRequest {
                transaction_digest: digests[0].transaction,
            })
            .await
            .unwrap();
        assert!(result.certified_transaction.is_some());
        let result = client
            .handle_transaction_info_request(TransactionInfoRequest {
                transaction_digest: digests[1].transaction,
            })
            .await
            .unwrap();
        assert!(result.certified_transaction.is_none());
    }

    // 3 (quorum) of the validators have executed 2 transactions,
    // and one validator has now executed 1 transaction through gossip (but not 2, because the
    // other transaction is now gone in the system).
    let all_seq = states
        .iter()
        .map(|s| s.database.next_sequence_number().unwrap());
    assert_eq!(all_seq.clone().filter(|s| s == &2).count(), 3,);
    assert_eq!(all_seq.filter(|s| s == &1).count(), 1,);

    // 3 (quorum) validator should still have 2 tx + 1 batch in the system,
    // while one validator (since it never see the second tx) only has 1 tx + 1 batch.
    let mut all_batch_item_counts = vec![];
    for state in &states {
        all_batch_item_counts.push(
            state
                .handle_batch_info_request(BatchInfoRequest {
                    start: Some(0),
                    length: 2,
                })
                .await
                .unwrap()
                .0
                .len(),
        );
    }
    assert_eq!(all_batch_item_counts.iter().filter(|c| *c == &3).count(), 3);
    assert_eq!(all_batch_item_counts.iter().filter(|c| *c == &2).count(), 1);
}

async fn start_gossip_process(
    states: Vec<Arc<AuthorityState>>,
    net: AuthorityAggregator<ConfigurableBatchActionClient>,
) -> Vec<JoinHandle<()>> {
    let mut active_authorities = Vec::new();

    // Start active processes.
    for state in states {
        let inner_net = net.clone();

        let handle = tokio::task::spawn(async move {
            let active_state =
                Arc::new(ActiveAuthority::new_with_ephemeral_storage(state, inner_net).unwrap());
            active_state.spawn_gossip_process(3).await;
        });
        active_authorities.push(handle);
    }

    active_authorities
}
