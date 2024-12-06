// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::{eth_sui_bridge, EthSuiBridge};
use crate::client::bridge_authority_aggregator::BridgeAuthorityAggregator;
use crate::crypto::BridgeAuthorityKeyPair;
use crate::e2e_tests::test_utils::TestClusterWrapperBuilder;
use crate::e2e_tests::test_utils::{
    get_signatures, initiate_bridge_erc20_to_sui, initiate_bridge_eth_to_sui,
    initiate_bridge_sui_to_eth, send_eth_tx_and_get_tx_receipt, BridgeTestClusterBuilder,
};
use crate::eth_transaction_builder::build_eth_transaction;
use crate::events::{
    SuiBridgeEvent, SuiToEthTokenBridgeV1, TokenTransferApproved, TokenTransferClaimed,
};
use crate::sui_transaction_builder::build_add_tokens_on_sui_transaction;
use crate::types::{AddTokensOnEvmAction, BridgeAction};
use crate::utils::publish_and_register_coins_return_add_coins_on_sui_action;
use crate::BRIDGE_ENABLE_PROTOCOL_VERSION;
use ethers::prelude::*;
use ethers::types::Address as EthAddress;
use std::collections::HashSet;
use sui_json_rpc_api::BridgeReadApiClient;
use sui_types::crypto::get_key_pair;
use test_cluster::TestClusterBuilder;

use std::path::Path;

use std::sync::Arc;
use sui_json_rpc_types::{SuiExecutionStatus, SuiTransactionBlockEffectsAPI};
use sui_types::bridge::{
    get_bridge, BridgeChainId, BridgeTokenMetadata, BridgeTrait, TOKEN_ID_ETH,
};
use sui_types::SUI_BRIDGE_OBJECT_ID;
use tracing::info;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_bridge_from_eth_to_sui_to_eth() {
    telemetry_subscribers::init_for_testing();

    let eth_chain_id = BridgeChainId::EthCustom as u8;
    let sui_chain_id = BridgeChainId::SuiCustom as u8;
    let timer = std::time::Instant::now();
    let mut bridge_test_cluster = BridgeTestClusterBuilder::new()
        .with_eth_env(true)
        .with_bridge_cluster(true)
        .with_num_validators(3)
        .build()
        .await;
    info!(
        "[Timer] Bridge test cluster started in {:?}",
        timer.elapsed()
    );
    let timer = std::time::Instant::now();
    let (eth_signer, _) = bridge_test_cluster
        .get_eth_signer_and_address()
        .await
        .unwrap();

    let sui_address = bridge_test_cluster.sui_user_address();
    let amount = 42;
    let sui_amount = amount * 100_000_000;

    initiate_bridge_eth_to_sui(&bridge_test_cluster, amount, 0)
        .await
        .unwrap();
    let events = bridge_test_cluster
        .new_bridge_events(
            HashSet::from_iter([
                TokenTransferApproved.get().unwrap().clone(),
                TokenTransferClaimed.get().unwrap().clone(),
            ]),
            true,
        )
        .await;
    // There are exactly 1 approved and 1 claimed event
    assert_eq!(events.len(), 2);

    let eth_coin = bridge_test_cluster
        .sui_client()
        .coin_read_api()
        .get_all_coins(sui_address, None, None)
        .await
        .unwrap()
        .data
        .iter()
        .find(|c| c.coin_type.contains("ETH"))
        .expect("Recipient should have received ETH coin now")
        .clone();
    assert_eq!(eth_coin.balance, sui_amount);
    info!(
        "[Timer] Eth to Sui bridge transfer finished in {:?}",
        timer.elapsed()
    );
    let timer = std::time::Instant::now();

    // Now let the recipient send the coin back to ETH
    let eth_address_1 = EthAddress::random();
    let nonce = 0;

    let sui_to_eth_bridge_action = initiate_bridge_sui_to_eth(
        &bridge_test_cluster,
        eth_address_1,
        eth_coin.object_ref(),
        nonce,
        sui_amount,
    )
    .await
    .unwrap();
    let events = bridge_test_cluster
        .new_bridge_events(
            HashSet::from_iter([
                SuiToEthTokenBridgeV1.get().unwrap().clone(),
                TokenTransferApproved.get().unwrap().clone(),
                TokenTransferClaimed.get().unwrap().clone(),
            ]),
            true,
        )
        .await;
    // There are exactly 1 deposit and 1 approved event
    assert_eq!(events.len(), 2);
    info!(
        "[Timer] Sui to Eth bridge transfer approved in {:?}",
        timer.elapsed()
    );
    let timer = std::time::Instant::now();

    // Test `get_parsed_token_transfer_message`
    let parsed_msg = bridge_test_cluster
        .bridge_client()
        .get_parsed_token_transfer_message(sui_chain_id, nonce)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(parsed_msg.source_chain as u8, sui_chain_id);
    assert_eq!(parsed_msg.seq_num, nonce);
    assert_eq!(
        parsed_msg.parsed_payload.sender_address,
        sui_address.to_vec()
    );
    assert_eq!(
        &parsed_msg.parsed_payload.target_address,
        eth_address_1.as_bytes()
    );
    assert_eq!(parsed_msg.parsed_payload.target_chain, eth_chain_id);
    assert_eq!(parsed_msg.parsed_payload.token_type, TOKEN_ID_ETH);
    assert_eq!(parsed_msg.parsed_payload.amount, sui_amount);

    let message = eth_sui_bridge::Message::from(sui_to_eth_bridge_action);
    let signatures = get_signatures(bridge_test_cluster.bridge_client(), nonce, sui_chain_id).await;

    let eth_sui_bridge = EthSuiBridge::new(
        bridge_test_cluster.contracts().sui_bridge,
        eth_signer.clone().into(),
    );
    let call = eth_sui_bridge.transfer_bridged_tokens_with_signatures(signatures, message);
    let eth_claim_tx_receipt = send_eth_tx_and_get_tx_receipt(call).await;
    assert_eq!(eth_claim_tx_receipt.status.unwrap().as_u64(), 1);
    info!(
        "[Timer] Sui to Eth bridge transfer claimed in {:?}",
        timer.elapsed()
    );
    // Assert eth_address_1 has received ETH
    assert_eq!(
        eth_signer.get_balance(eth_address_1, None).await.unwrap(),
        U256::from(amount) * U256::exp10(18)
    );
}

// Test add new coins on both Sui and Eth
// Also test bridge ndoe handling `NewTokenEvent``
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_add_new_coins_on_sui_and_eth() {
    telemetry_subscribers::init_for_testing();
    let mut bridge_test_cluster = BridgeTestClusterBuilder::new()
        .with_eth_env(true)
        .with_bridge_cluster(false)
        .with_num_validators(3)
        .build()
        .await;
    let bridge_arg = bridge_test_cluster.get_mut_bridge_arg().await.unwrap();

    // Register tokens on Sui
    let token_id = 5;
    let token_sui_decimal = 9; // this needs to match ka.move
    let token_price = 10000;
    let sender = bridge_test_cluster.sui_user_address();
    info!("Published new token");
    let sui_action = publish_and_register_coins_return_add_coins_on_sui_action(
        bridge_test_cluster.wallet(),
        bridge_arg,
        vec![Path::new("../../bridge/move/tokens/mock/ka").into()],
        vec![token_id],
        vec![token_price],
        1, // seq num
    )
    .await;
    let new_token_erc_address = bridge_test_cluster.contracts().ka;
    let eth_action = BridgeAction::AddTokensOnEvmAction(AddTokensOnEvmAction {
        nonce: 0,
        chain_id: BridgeChainId::EthCustom,
        native: true,
        token_ids: vec![token_id],
        token_addresses: vec![new_token_erc_address],
        token_sui_decimals: vec![token_sui_decimal],
        token_prices: vec![token_price],
    });

    info!("Starting bridge cluster");

    bridge_test_cluster.set_approved_governance_actions_for_next_start(vec![
        vec![sui_action.clone(), eth_action.clone()],
        vec![sui_action.clone()],
        vec![eth_action.clone()],
    ]);
    bridge_test_cluster.start_bridge_cluster().await;
    bridge_test_cluster
        .wait_for_bridge_cluster_to_be_up(10)
        .await;
    info!("Bridge cluster is up");

    let bridge_committee = Arc::new(
        bridge_test_cluster
            .bridge_client()
            .get_bridge_committee()
            .await
            .expect("Failed to get bridge committee"),
    );
    let agg = BridgeAuthorityAggregator::new_for_testing(bridge_committee);
    let certified_sui_action = agg
        .request_committee_signatures(sui_action)
        .await
        .expect("Failed to request committee signatures for AddTokensOnSuiAction");
    let certified_eth_action = agg
        .request_committee_signatures(eth_action.clone())
        .await
        .expect("Failed to request committee signatures for AddTokensOnEvmAction");

    let tx = build_add_tokens_on_sui_transaction(
        sender,
        &bridge_test_cluster
            .wallet()
            .get_one_gas_object_owned_by_address(sender)
            .await
            .unwrap()
            .unwrap(),
        certified_sui_action,
        bridge_arg,
        1000,
    )
    .unwrap();

    let response = bridge_test_cluster.sign_and_execute_transaction(&tx).await;
    let effects = response.effects.unwrap();
    assert_eq!(effects.status(), &SuiExecutionStatus::Success);
    assert!(response.events.unwrap().data.iter().any(|e| {
        let sui_bridge_event = SuiBridgeEvent::try_from_sui_event(e).unwrap().unwrap();
        match sui_bridge_event {
            SuiBridgeEvent::NewTokenEvent(e) => {
                assert_eq!(e.token_id, token_id);
                true
            }
            _ => false,
        }
    }));
    info!("Approved new token on Sui");

    // Assert new token is correctly added
    let treasury_summary = bridge_test_cluster
        .bridge_client()
        .get_treasury_summary()
        .await
        .unwrap();
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

    // Add new token on EVM
    let config_address = bridge_test_cluster.contracts().bridge_config;
    let eth_signer = bridge_test_cluster.get_eth_signer().await;
    let eth_call = build_eth_transaction(config_address, eth_signer, certified_eth_action)
        .await
        .unwrap();
    let eth_receipt = send_eth_tx_and_get_tx_receipt(eth_call).await;
    assert_eq!(eth_receipt.status.unwrap().as_u64(), 1);

    // Verify new tokens are added on EVM
    let (address, dp, price) = bridge_test_cluster
        .eth_env()
        .get_supported_token(token_id)
        .await;
    assert_eq!(address, new_token_erc_address);
    assert_eq!(dp, 9);
    assert_eq!(price, token_price);

    initiate_bridge_erc20_to_sui(
        &bridge_test_cluster,
        100,
        new_token_erc_address,
        token_id,
        0,
    )
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_create_bridge_state_object() {
    let test_cluster = TestClusterBuilder::new()
        .with_protocol_version((BRIDGE_ENABLE_PROTOCOL_VERSION - 1).into())
        .with_epoch_duration_ms(20000)
        .build()
        .await;

    let handles = test_cluster.all_node_handles();

    // no node has the bridge state object yet
    for h in &handles {
        h.with(|node| {
            assert!(node
                .state()
                .get_object_cache_reader()
                .get_latest_object_ref_or_tombstone(SUI_BRIDGE_OBJECT_ID)
                .is_none());
        });
    }

    // wait until feature is enabled
    test_cluster
        .wait_for_protocol_version(BRIDGE_ENABLE_PROTOCOL_VERSION.into())
        .await;
    // wait until next epoch - authenticator state object is created at the end of the first epoch
    // in which it is supported.
    test_cluster.wait_for_epoch_all_nodes(2).await; // protocol upgrade completes in epoch 1

    for h in &handles {
        h.with(|node| {
            node.state()
                .get_object_cache_reader()
                .get_latest_object_ref_or_tombstone(SUI_BRIDGE_OBJECT_ID)
                .expect("auth state object should exist");
        });
    }
}

#[tokio::test]
async fn test_committee_registration() {
    telemetry_subscribers::init_for_testing();
    let mut bridge_keys = vec![];
    for _ in 0..=3 {
        let (_, kp): (_, BridgeAuthorityKeyPair) = get_key_pair();
        bridge_keys.push(kp);
    }
    let test_cluster = TestClusterWrapperBuilder::new()
        .with_bridge_authority_keys(bridge_keys)
        .build()
        .await;

    let bridge = get_bridge(
        test_cluster
            .inner
            .fullnode_handle
            .sui_node
            .state()
            .get_object_store(),
    )
    .unwrap();

    // Member should be empty before end of epoch
    assert!(bridge.committee().members.contents.is_empty());
    assert_eq!(
        test_cluster.inner.swarm.active_validators().count(),
        bridge.committee().member_registrations.contents.len()
    );

    test_cluster
        .trigger_reconfiguration_if_not_yet_and_assert_bridge_committee_initialized()
        .await;
}

#[tokio::test]
async fn test_bridge_api_compatibility() {
    let test_cluster: test_cluster::TestCluster = TestClusterBuilder::new()
        .with_protocol_version(BRIDGE_ENABLE_PROTOCOL_VERSION.into())
        .build()
        .await;

    test_cluster.trigger_reconfiguration().await;
    let client = test_cluster.rpc_client();
    client.get_latest_bridge().await.unwrap();
    // TODO: assert fields in summary

    client
        .get_bridge_object_initial_shared_version()
        .await
        .unwrap();
}
