// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::client::bridge_authority_aggregator::BridgeAuthorityAggregator;
use crate::e2e_tests::test_utils::initialize_bridge_environment;
use crate::e2e_tests::test_utils::{
    publish_coins_return_add_coins_on_sui_action, start_bridge_cluster,
};
use crate::sui_client::SuiBridgeClient;
use crate::sui_transaction_builder::build_add_tokens_on_sui_transaction;

use std::path::Path;

use std::sync::Arc;
use sui_json_rpc_types::{SuiExecutionStatus, SuiTransactionBlockEffectsAPI};
use sui_types::bridge::BridgeTokenMetadata;

use tracing::info;

#[tokio::test]
async fn test_add_new_coins_on_sui() {
    telemetry_subscribers::init_for_testing();

    let (mut test_cluster, bridge_cluster, eth_environment) = initialize_bridge_environment().await;

    drop(bridge_cluster);

    let bridge_arg = test_cluster.get_mut_bridge_arg().await.unwrap();

    // Register tokens
    let token_id = 42;
    let token_price = 10000;
    let sender = test_cluster.get_address_0();
    let tx = test_cluster
        .test_transaction_builder_with_sender(sender)
        .await
        .publish(Path::new("../../bridge/move/tokens/mock/ka").into())
        .build();
    let publish_token_response = test_cluster.sign_and_execute_transaction(&tx).await;
    info!("Published new token");
    let action = publish_coins_return_add_coins_on_sui_action(
        test_cluster.wallet_mut(),
        bridge_arg,
        vec![publish_token_response],
        vec![token_id],
        vec![token_price],
        1, // seq num
    )
    .await;

    // TODO: do not block on `build_with_bridge`, return with bridge keys immediately
    // to paralyze the setup.
    let sui_bridge_client = SuiBridgeClient::new(&test_cluster.fullnode_handle.rpc_url)
        .await
        .unwrap();

    info!("Starting bridge cluster");

    // kill bridge cluster
    start_bridge_cluster(
        &test_cluster,
        &eth_environment,
        vec![
            vec![action.clone()],
            vec![action.clone()],
            vec![action.clone()],
            vec![],
        ],
    )
    .await;

    test_cluster.wait_for_bridge_cluster_to_be_up(10).await;
    info!("Bridge cluster is up");
    let bridge_committee = Arc::new(
        sui_bridge_client
            .get_bridge_committee()
            .await
            .expect("Failed to get bridge committee"),
    );
    let agg = BridgeAuthorityAggregator::new(bridge_committee);
    let threshold = action.approval_threshold();
    let certified_action = agg
        .request_committee_signatures(action, threshold)
        .await
        .expect("Failed to request committee signatures");

    let tx = build_add_tokens_on_sui_transaction(
        sender,
        &test_cluster
            .wallet
            .get_one_gas_object_owned_by_address(sender)
            .await
            .unwrap()
            .unwrap(),
        certified_action,
        bridge_arg,
    )
    .unwrap();

    let response = test_cluster.sign_and_execute_transaction(&tx).await;
    assert_eq!(
        response.effects.unwrap().status(),
        &SuiExecutionStatus::Success
    );
    info!("Approved new token");

    // Assert new token is correctly added
    let treasury_summary = sui_bridge_client.get_treasury_summary().await.unwrap();
    assert_eq!(treasury_summary.id_token_type_map.len(), 5); // 4 + 1 new token
    let (id, _type) = treasury_summary
        .id_token_type_map
        .iter()
        .find(|(id, _)| id == &token_id)
        .unwrap();
    let (_type, metadata) = treasury_summary
        .supported_tokens
        .iter()
        .find(|(_type_, _)| _type == _type_)
        .unwrap();
    assert_eq!(
        metadata,
        &BridgeTokenMetadata {
            id: *id,
            decimal_multiplier: 1_000_000_000,
            notional_value: token_price,
            native_token: false,
        }
    );
}

// #[tokio::test]
// async fn test_update_committee_blocklist() {
//     telemetry_subscribers::init_for_testing();

//     let (mut test_cluster, eth_environment) = initialize_bridge_environment().await;

//     // build the update blocklist action

//     // Question: does this governance action require to messages to be signed by the committee?
// }

// #[tokio::test]
// async fn test_emergency_op() {
//     telemetry_subscribers::init_for_testing();

//     let (mut test_cluster, eth_environment) = initialize_bridge_environment().await;
// }

// #[tokio::test]
// async fn test_update_bridge_limit() {
//     telemetry_subscribers::init_for_testing();

//     let (mut test_cluster, eth_environment) = initialize_bridge_environment().await;
// }

// #[tokio::test]
// async fn test_update_token_price() {
//     telemetry_subscribers::init_for_testing();

//     let (mut test_cluster, eth_environment) = initialize_bridge_environment().await;
// }
