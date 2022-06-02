// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority_active::gossip::configurable_batch_action_client::{
    init_configurable_authorities, BatchAction,
};
use crate::authority_active::{LifecycleSignal, LifecycleSignalSender, MAX_RETRY_DELAY_MS};
use std::time::Duration;

#[tokio::test(flavor = "current_thread", start_paused = true)]
pub async fn test_gossip() {
    let action_sequence = vec![
        BatchAction::EmitUpdateItem(),
        BatchAction::EmitUpdateItem(),
        BatchAction::EmitUpdateItem(),
    ];

    let (clients, states, digests) = init_configurable_authorities(action_sequence).await;

    let mut active_authorities = Vec::new();
    // Start active processes.

    let mut control_channel_senders = Vec::new();
    for state in states.clone() {
        let inner_state = state.clone();
        let inner_clients = clients.clone();

        let (_sender, receiver) = LifecycleSignalSender::new();
        let handle = tokio::task::spawn(async move {
            let active_state = ActiveAuthority::new(inner_state, inner_clients).unwrap();
            active_state.spawn_all_active_processes(receiver).await;
        });
        _sender.signal(LifecycleSignal::Start).await;

        control_channel_senders.push(_sender);
        active_authorities.push(handle);
    }
    tokio::time::sleep(Duration::from_secs(20)).await;

    // Expected outcome of gossip: each digest's tx signature and cert is now on every authority.
    let clients_final: Vec<_> = clients.values().collect();
    for client in clients_final.iter() {
        for digest in &digests {
            let result1 = client
                .handle_transaction_info_request(TransactionInfoRequest {
                    transaction_digest: *digest,
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

    let (clients, states, digests) = init_configurable_authorities(action_sequence).await;

    let mut active_authorities = Vec::new();

    // Start active processes.
    let mut control_channel_senders = Vec::new();
    for state in states.clone() {
        let inner_state = state.clone();
        let inner_clients = clients.clone();

        let (_sender, receiver) = LifecycleSignalSender::new();
        let handle = tokio::task::spawn(async move {
            let active_state = ActiveAuthority::new(inner_state, inner_clients).unwrap();
            active_state.spawn_all_active_processes(receiver).await;
        });
        _sender.signal(LifecycleSignal::Start).await;
        control_channel_senders.push(_sender);
        active_authorities.push(handle);
    }
    // failure back-offs were set from the errors
    tokio::time::sleep(Duration::from_millis(MAX_RETRY_DELAY_MS)).await;

    let clients_final: Vec<_> = clients.values().collect();
    for client in clients_final.iter() {
        for digest in &digests {
            let result1 = client
                .handle_transaction_info_request(TransactionInfoRequest {
                    transaction_digest: *digest,
                })
                .await;

            assert!(result1.is_ok());
            let result = result1.unwrap();
            let found_cert = result.certified_transaction.is_some();
            assert!(found_cert);
        }
    }
}
