// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::{eth_sui_bridge, EthBridgeEvent, EthSuiBridge};
use crate::client::bridge_authority_aggregator::BridgeAuthorityAggregator;
use crate::e2e_tests::test_utils::BridgeTestCluster;
use crate::e2e_tests::test_utils::{get_signatures, BridgeTestClusterBuilder};
use crate::events::{
    SuiBridgeEvent, SuiToEthTokenBridgeV1, TokenTransferApproved, TokenTransferClaimed,
};
use crate::sui_client::SuiBridgeClient;
use crate::sui_transaction_builder::build_add_tokens_on_sui_transaction;
use crate::types::{BridgeAction, BridgeActionStatus, SuiToEthBridgeAction};
use crate::utils::publish_and_register_coins_return_add_coins_on_sui_action;
use crate::utils::EthSigner;
use eth_sui_bridge::EthSuiBridgeEvents;
use ethers::prelude::*;
use ethers::types::Address as EthAddress;
use move_core_types::ident_str;
use std::collections::{HashMap, HashSet};

use std::path::Path;

use anyhow::anyhow;
use std::sync::Arc;
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse,
};
use sui_sdk::wallet_context::WalletContext;
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::bridge::{BridgeChainId, BridgeTokenMetadata, BRIDGE_MODULE_NAME, TOKEN_ID_ETH};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{ObjectArg, TransactionData};
use sui_types::{TypeTag, BRIDGE_PACKAGE_ID};
use tap::TapFallible;
use tracing::info;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_bridge_from_eth_to_sui_to_eth() {
    telemetry_subscribers::init_for_testing();

    let eth_chain_id = BridgeChainId::EthCustom as u8;
    let sui_chain_id = BridgeChainId::SuiCustom as u8;

    let mut bridge_test_cluster = BridgeTestClusterBuilder::new()
        .with_eth_env(true)
        .with_bridge_cluster(true)
        .build()
        .await;

    let (eth_signer, _) = bridge_test_cluster
        .get_eth_signer_and_address()
        .await
        .unwrap();

    let sui_address = bridge_test_cluster.sui_user_address();
    let amount = 42;
    let sui_amount = amount * 100_000_000;

    initiate_bridge_eth_to_sui(&bridge_test_cluster, amount, sui_amount, TOKEN_ID_ETH, 0)
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
    let tx = eth_sui_bridge.transfer_bridged_tokens_with_signatures(signatures, message);
    let _eth_claim_tx_receipt = tx.send().await.unwrap().await.unwrap().unwrap();
    info!("Sui to Eth bridge transfer claimed");
    // Assert eth_address_1 has received ETH
    assert_eq!(
        eth_signer.get_balance(eth_address_1, None).await.unwrap(),
        U256::from(amount) * U256::exp10(18)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_add_new_coins_on_sui() {
    telemetry_subscribers::init_for_testing();
    let mut bridge_test_cluster = BridgeTestClusterBuilder::new()
        .with_eth_env(true)
        .with_bridge_cluster(false)
        .build()
        .await;

    let bridge_arg = bridge_test_cluster.get_mut_bridge_arg().await.unwrap();

    // Register tokens
    let token_id = 42;
    let token_price = 10000;
    let sender = bridge_test_cluster.sui_user_address();
    info!("Published new token");
    let action = publish_and_register_coins_return_add_coins_on_sui_action(
        bridge_test_cluster.wallet(),
        bridge_arg,
        vec![Path::new("../../bridge/move/tokens/mock/ka").into()],
        vec![token_id],
        vec![token_price],
        1, // seq num
    )
    .await;

    info!("Starting bridge cluster");

    bridge_test_cluster.set_approved_governance_actions_for_next_start(vec![
        vec![action.clone()],
        vec![action.clone()],
        vec![action.clone()],
        vec![],
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
    let agg = BridgeAuthorityAggregator::new(bridge_committee);
    let certified_action = agg
        .request_committee_signatures(action)
        .await
        .expect("Failed to request committee signatures");

    let tx = build_add_tokens_on_sui_transaction(
        sender,
        &bridge_test_cluster
            .wallet()
            .get_one_gas_object_owned_by_address(sender)
            .await
            .unwrap()
            .unwrap(),
        certified_action,
        bridge_arg,
        1000,
    )
    .unwrap();

    let response = bridge_test_cluster.sign_and_execute_transaction(&tx).await;
    assert_eq!(
        response.effects.unwrap().status(),
        &SuiExecutionStatus::Success
    );
    info!("Approved new token");

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

pub async fn initiate_bridge_eth_to_sui(
    bridge_test_cluster: &BridgeTestCluster,
    amount: u64,
    sui_amount: u64,
    token_id: u8,
    nonce: u64,
) -> Result<(), anyhow::Error> {
    info!("Depositing Eth to Solidity contract");
    let (eth_signer, eth_address) = bridge_test_cluster
        .get_eth_signer_and_address()
        .await
        .unwrap();

    let sui_address = bridge_test_cluster.sui_user_address();
    let sui_chain_id = bridge_test_cluster.sui_chain_id();
    let eth_chain_id = bridge_test_cluster.eth_chain_id();

    let eth_tx = deposit_native_eth_to_sol_contract(
        &eth_signer,
        bridge_test_cluster.contracts().sui_bridge,
        sui_address,
        sui_chain_id,
        amount,
    )
    .await;
    let pending_tx = eth_tx.send().await.unwrap();
    let tx_receipt = pending_tx.await.unwrap().unwrap();
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
    info!("Deposited Eth to Solidity contract");

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
    loop {
        let res = sui_bridge_client
            .get_token_transfer_action_onchain_status_until_success(chain_id as u8, nonce)
            .await;
        if res == status {
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
