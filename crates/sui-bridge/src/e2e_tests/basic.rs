// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::{eth_sui_bridge, EthBridgeEvent, EthERC20, EthSuiBridge};
use crate::client::bridge_authority_aggregator::BridgeAuthorityAggregator;
use crate::crypto::BridgeAuthorityKeyPair;
use crate::e2e_tests::test_utils::{
    get_signatures, send_eth_tx_and_get_tx_receipt, BridgeTestClusterBuilder,
};
use crate::e2e_tests::test_utils::{BridgeTestCluster, TestClusterWrapperBuilder};
use crate::eth_transaction_builder::build_eth_transaction;
use crate::events::{
    SuiBridgeEvent, SuiToEthTokenBridgeV1, TokenTransferApproved, TokenTransferClaimed,
};
use crate::sui_client::SuiBridgeClient;
use crate::sui_transaction_builder::build_add_tokens_on_sui_transaction;
use crate::types::{AddTokensOnEvmAction, BridgeAction, BridgeActionStatus, SuiToEthBridgeAction};
use crate::utils::publish_and_register_coins_return_add_coins_on_sui_action;
use crate::utils::EthSigner;
use crate::BRIDGE_ENABLE_PROTOCOL_VERSION;
use eth_sui_bridge::EthSuiBridgeEvents;
use ethers::prelude::*;
use ethers::types::Address as EthAddress;
use move_core_types::ident_str;
use std::collections::{HashMap, HashSet};
use sui_json_rpc_api::BridgeReadApiClient;
use sui_types::crypto::get_key_pair;
use test_cluster::TestClusterBuilder;

use std::path::Path;

use anyhow::anyhow;
use std::sync::Arc;
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse,
};
use sui_sdk::wallet_context::WalletContext;
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::bridge::{
    get_bridge, BridgeChainId, BridgeTokenMetadata, BridgeTrait, BRIDGE_MODULE_NAME, TOKEN_ID_ETH,
};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{ObjectArg, TransactionData};
use sui_types::{TypeTag, BRIDGE_PACKAGE_ID, SUI_BRIDGE_OBJECT_ID};
use tap::TapFallible;
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
                .unwrap()
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
                .unwrap()
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

pub(crate) async fn deposit_native_eth_to_sol_contract(
    signer: &EthSigner,
    contract_address: EthAddress,
    sui_recipient_address: SuiAddress,
    sui_chain_id: BridgeChainId,
    amount: u64,
) -> ContractCall<EthSigner, ()> {
    let contract = EthSuiBridge::new(contract_address, signer.clone().into());
    let sui_recipient_address = sui_recipient_address.to_vec().into();
    let amount = U256::from(amount) * U256::exp10(18); // 1 ETH
    contract
        .bridge_eth(sui_recipient_address, sui_chain_id as u8)
        .value(amount)
}

async fn deposit_eth_to_sui_package(
    sui_client: &SuiClient,
    sui_address: SuiAddress,
    wallet_context: &WalletContext,
    target_chain: BridgeChainId,
    target_address: EthAddress,
    token: ObjectRef,
    bridge_object_arg: ObjectArg,
    sui_token_type_tags: &HashMap<u8, TypeTag>,
) -> Result<SuiTransactionBlockResponse, anyhow::Error> {
    let mut builder = ProgrammableTransactionBuilder::new();
    let arg_target_chain = builder.pure(target_chain as u8).unwrap();
    let arg_target_address = builder.pure(target_address.as_bytes()).unwrap();
    let arg_token = builder.obj(ObjectArg::ImmOrOwnedObject(token)).unwrap();
    let arg_bridge = builder.obj(bridge_object_arg).unwrap();

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        BRIDGE_MODULE_NAME.to_owned(),
        ident_str!("send_token").to_owned(),
        vec![sui_token_type_tags.get(&TOKEN_ID_ETH).unwrap().clone()],
        vec![arg_bridge, arg_target_chain, arg_target_address, arg_token],
    );

    let pt = builder.finish();
    let gas_object_ref = wallet_context
        .get_one_gas_object_owned_by_address(sui_address)
        .await
        .unwrap()
        .unwrap();
    let tx_data = TransactionData::new_programmable(
        sui_address,
        vec![gas_object_ref],
        pt,
        500_000_000,
        sui_client
            .governance_api()
            .get_reference_gas_price()
            .await
            .unwrap(),
    );
    let tx = wallet_context.sign_transaction(&tx_data);
    wallet_context.execute_transaction_may_fail(tx).await
}

pub async fn initiate_bridge_erc20_to_sui(
    bridge_test_cluster: &BridgeTestCluster,
    amount_u64: u64,
    token_address: EthAddress,
    token_id: u8,
    nonce: u64,
) -> Result<(), anyhow::Error> {
    let (eth_signer, eth_address) = bridge_test_cluster
        .get_eth_signer_and_address()
        .await
        .unwrap();

    // First, mint ERC20 tokens to the signer
    let contract = EthERC20::new(token_address, eth_signer.clone().into());
    let decimal = contract.decimals().await? as usize;
    let amount = U256::from(amount_u64) * U256::exp10(decimal);
    let sui_amount = amount.as_u64();
    let mint_call = contract.mint(eth_address, amount);
    let mint_tx_receipt = send_eth_tx_and_get_tx_receipt(mint_call).await;
    assert_eq!(mint_tx_receipt.status.unwrap().as_u64(), 1);

    // Second, set allowance
    let allowance_call = contract.approve(bridge_test_cluster.contracts().sui_bridge, amount);
    let allowance_tx_receipt = send_eth_tx_and_get_tx_receipt(allowance_call).await;
    assert_eq!(allowance_tx_receipt.status.unwrap().as_u64(), 1);

    // Third, deposit to bridge
    let sui_recipient_address = bridge_test_cluster.sui_user_address();
    let sui_chain_id = bridge_test_cluster.sui_chain_id();
    let eth_chain_id = bridge_test_cluster.eth_chain_id();

    info!(
        "Depositing ERC20 (token id:{}, token_address: {}) to Solidity contract",
        token_id, token_address
    );
    let contract = EthSuiBridge::new(
        bridge_test_cluster.contracts().sui_bridge,
        eth_signer.clone().into(),
    );
    let deposit_call = contract.bridge_erc20(
        token_id,
        amount,
        sui_recipient_address.to_vec().into(),
        sui_chain_id as u8,
    );
    let tx_receipt = send_eth_tx_and_get_tx_receipt(deposit_call).await;
    let eth_bridge_event = tx_receipt
        .logs
        .iter()
        .find_map(EthBridgeEvent::try_from_log)
        .unwrap();
    let EthBridgeEvent::EthSuiBridgeEvents(EthSuiBridgeEvents::TokensDepositedFilter(
        eth_bridge_event,
    )) = eth_bridge_event
    else {
        unreachable!();
    };
    // assert eth log matches
    assert_eq!(eth_bridge_event.source_chain_id, eth_chain_id as u8);
    assert_eq!(eth_bridge_event.nonce, nonce);
    assert_eq!(eth_bridge_event.destination_chain_id, sui_chain_id as u8);
    assert_eq!(eth_bridge_event.token_id, token_id);
    assert_eq!(eth_bridge_event.sui_adjusted_amount, sui_amount);
    assert_eq!(eth_bridge_event.sender_address, eth_address);
    assert_eq!(
        eth_bridge_event.recipient_address,
        sui_recipient_address.to_vec()
    );
    info!(
        "Deposited ERC20 (token id:{}, token_address: {}) to Solidity contract",
        token_id, token_address
    );

    wait_for_transfer_action_status(
        bridge_test_cluster.bridge_client(),
        eth_chain_id,
        nonce,
        BridgeActionStatus::Claimed,
    )
    .await
    .tap_ok(|_| {
        info!(
            nonce,
            token_id, amount_u64, "Eth to Sui bridge transfer claimed"
        );
    })
}

pub async fn initiate_bridge_eth_to_sui(
    bridge_test_cluster: &BridgeTestCluster,
    amount: u64,
    nonce: u64,
) -> Result<(), anyhow::Error> {
    info!("Depositing native Ether to Solidity contract, nonce: {nonce}, amount: {amount}");
    let (eth_signer, eth_address) = bridge_test_cluster
        .get_eth_signer_and_address()
        .await
        .unwrap();

    let sui_address = bridge_test_cluster.sui_user_address();
    let sui_chain_id = bridge_test_cluster.sui_chain_id();
    let eth_chain_id = bridge_test_cluster.eth_chain_id();
    let token_id = TOKEN_ID_ETH;

    let sui_amount = (U256::from(amount) * U256::exp10(8)).as_u64(); // DP for Ether on Sui

    let eth_tx = deposit_native_eth_to_sol_contract(
        &eth_signer,
        bridge_test_cluster.contracts().sui_bridge,
        sui_address,
        sui_chain_id,
        amount,
    )
    .await;
    let tx_receipt = send_eth_tx_and_get_tx_receipt(eth_tx).await;
    let eth_bridge_event = tx_receipt
        .logs
        .iter()
        .find_map(EthBridgeEvent::try_from_log)
        .unwrap();
    let EthBridgeEvent::EthSuiBridgeEvents(EthSuiBridgeEvents::TokensDepositedFilter(
        eth_bridge_event,
    )) = eth_bridge_event
    else {
        unreachable!();
    };
    // assert eth log matches
    assert_eq!(eth_bridge_event.source_chain_id, eth_chain_id as u8);
    assert_eq!(eth_bridge_event.nonce, nonce);
    assert_eq!(eth_bridge_event.destination_chain_id, sui_chain_id as u8);
    assert_eq!(eth_bridge_event.token_id, token_id);
    assert_eq!(eth_bridge_event.sui_adjusted_amount, sui_amount);
    assert_eq!(eth_bridge_event.sender_address, eth_address);
    assert_eq!(eth_bridge_event.recipient_address, sui_address.to_vec());
    info!(
        "Deposited Eth to Solidity contract, block: {:?}",
        tx_receipt.block_number
    );

    wait_for_transfer_action_status(
        bridge_test_cluster.bridge_client(),
        eth_chain_id,
        nonce,
        BridgeActionStatus::Claimed,
    )
    .await
    .tap_ok(|_| {
        info!("Eth to Sui bridge transfer claimed");
    })
}

pub async fn initiate_bridge_sui_to_eth(
    bridge_test_cluster: &BridgeTestCluster,
    eth_address: EthAddress,
    token: ObjectRef,
    nonce: u64,
    sui_amount: u64,
) -> Result<SuiToEthBridgeAction, anyhow::Error> {
    let bridge_object_arg = bridge_test_cluster
        .bridge_client()
        .get_mutable_bridge_object_arg_must_succeed()
        .await;
    let sui_client = bridge_test_cluster.sui_client();
    let token_types = bridge_test_cluster
        .bridge_client()
        .get_token_id_map()
        .await
        .unwrap();
    let sui_address = bridge_test_cluster.sui_user_address();

    let resp = match deposit_eth_to_sui_package(
        sui_client,
        sui_address,
        bridge_test_cluster.wallet(),
        bridge_test_cluster.eth_chain_id(),
        eth_address,
        token,
        bridge_object_arg,
        &token_types,
    )
    .await
    {
        Ok(resp) => {
            if !resp.status_ok().unwrap() {
                return Err(anyhow!("Sui TX error"));
            } else {
                resp
            }
        }
        Err(e) => return Err(e),
    };

    let sui_events = resp.events.unwrap().data;
    let bridge_event = sui_events
        .iter()
        .filter_map(|e| {
            let sui_bridge_event = SuiBridgeEvent::try_from_sui_event(e).unwrap()?;
            sui_bridge_event.try_into_bridge_action(e.id.tx_digest, e.id.event_seq as u16)
        })
        .find_map(|e| {
            if let BridgeAction::SuiToEthBridgeAction(a) = e {
                Some(a)
            } else {
                None
            }
        })
        .unwrap();
    info!("Deposited Eth to move package");
    assert_eq!(bridge_event.sui_bridge_event.nonce, nonce);
    assert_eq!(
        bridge_event.sui_bridge_event.sui_chain_id,
        bridge_test_cluster.sui_chain_id()
    );
    assert_eq!(
        bridge_event.sui_bridge_event.eth_chain_id,
        bridge_test_cluster.eth_chain_id()
    );
    assert_eq!(bridge_event.sui_bridge_event.sui_address, sui_address);
    assert_eq!(bridge_event.sui_bridge_event.eth_address, eth_address);
    assert_eq!(bridge_event.sui_bridge_event.token_id, TOKEN_ID_ETH);
    assert_eq!(
        bridge_event.sui_bridge_event.amount_sui_adjusted,
        sui_amount
    );

    // Wait for the bridge action to be approved
    wait_for_transfer_action_status(
        bridge_test_cluster.bridge_client(),
        bridge_test_cluster.sui_chain_id(),
        nonce,
        BridgeActionStatus::Approved,
    )
    .await
    .unwrap();
    info!("Sui to Eth bridge transfer approved.");

    Ok(bridge_event)
}

async fn wait_for_transfer_action_status(
    sui_bridge_client: &SuiBridgeClient,
    chain_id: BridgeChainId,
    nonce: u64,
    status: BridgeActionStatus,
) -> Result<(), anyhow::Error> {
    // Wait for the bridge action to be approved
    let now = std::time::Instant::now();
    info!(
        "Waiting for onchain status {:?}. chain: {:?}, nonce: {nonce}",
        status, chain_id as u8
    );
    loop {
        let timer = std::time::Instant::now();
        let res = sui_bridge_client
            .get_token_transfer_action_onchain_status_until_success(chain_id as u8, nonce)
            .await;
        info!(
            "get_token_transfer_action_onchain_status_until_success took {:?}, status: {:?}",
            timer.elapsed(),
            res
        );

        if res == status {
            info!(
                "detected on chain status {:?}. chain: {:?}, nonce: {nonce}",
                status, chain_id as u8
            );
            return Ok(());
        }
        if now.elapsed().as_secs() > 60 {
            return Err(anyhow!(
                "Timeout waiting for token transfer action to be {:?}. chain_id: {chain_id:?}, nonce: {nonce}. Time elapsed: {:?}",
                status,
                now.elapsed(),
            ));
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
