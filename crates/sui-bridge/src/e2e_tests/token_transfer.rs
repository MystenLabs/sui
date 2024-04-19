// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::{eth_sui_bridge, EthBridgeEvent, EthSuiBridge};
use crate::e2e_tests::test_utils::{get_signatures, initialize_bridge_environment, TEST_PK};
use crate::events::SuiBridgeEvent;
use crate::sui_client::SuiBridgeClient;
use crate::types::{BridgeAction, BridgeActionStatus, SuiToEthBridgeAction};
use crate::utils::EthSigner;
use eth_sui_bridge::EthSuiBridgeEvents;
use ethers::prelude::*;
use ethers::types::Address as EthAddress;
use move_core_types::ident_str;
use std::collections::HashMap;

use sui_json_rpc_types::SuiTransactionBlockResponse;
use sui_sdk::wallet_context::WalletContext;
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::bridge::{BridgeChainId, BRIDGE_MODULE_NAME, TOKEN_ID_ETH};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{ObjectArg, TransactionData};
use sui_types::{TypeTag, BRIDGE_PACKAGE_ID};
use tracing::info;

#[tokio::test]
async fn test_bridge_from_eth_to_sui_to_eth() {
    telemetry_subscribers::init_for_testing();

    let eth_chain_id = BridgeChainId::EthCustom as u8;
    let sui_chain_id = BridgeChainId::SuiCustom as u8;

    let (mut test_cluster, _bridge_cluster, eth_environment) =
        initialize_bridge_environment().await;

    let (eth_signer, _) = eth_environment.get_signer(TEST_PK).await.unwrap();

    let eth_address = eth_signer.address();

    let sui_client = test_cluster.fullnode_handle.sui_client.clone();

    let sui_bridge_client = SuiBridgeClient::new(&test_cluster.fullnode_handle.rpc_url)
        .await
        .unwrap();

    let sui_address = test_cluster.get_address_0();
    let amount = 42;
    // ETH coin has 8 decimals on Sui
    let sui_amount = amount * 100_000_000;

    initiate_bridge_eth_to_sui(
        &sui_bridge_client,
        &eth_signer,
        eth_environment.contracts().sui_bridge,
        sui_address,
        eth_address,
        eth_chain_id,
        sui_chain_id,
        amount,
        sui_amount,
        TOKEN_ID_ETH,
        0,
    )
    .await;
    info!("Deposited Eth to Sol contract");

    let eth_coin = sui_client
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
    let bridge_obj_arg = sui_bridge_client
        .get_mutable_bridge_object_arg_must_succeed()
        .await;
    let nonce = 0;

    let sui_token_type_tags = sui_bridge_client.get_token_id_map().await.unwrap();

    let sui_to_eth_bridge_action = initiate_bridge_sui_to_eth(
        &sui_bridge_client,
        &sui_client,
        sui_address,
        test_cluster.wallet_mut(),
        eth_chain_id,
        sui_chain_id,
        eth_address_1,
        eth_coin.object_ref(),
        nonce,
        bridge_obj_arg,
        sui_amount,
        &sui_token_type_tags,
    )
    .await;
    info!("Deposited Eth to move package");
    let message = eth_sui_bridge::Message::from(sui_to_eth_bridge_action);

    let signatures = get_signatures(
        &sui_bridge_client,
        nonce,
        sui_chain_id,
        &sui_client,
        message.message_type,
    )
    .await;

    let eth_sui_bridge = EthSuiBridge::new(
        eth_environment.contracts().sui_bridge,
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

pub(crate) async fn deposit_native_eth_to_sol_contract(
    signer: &EthSigner,
    contract_address: EthAddress,
    sui_recipient_address: SuiAddress,
    sui_chain_id: u8,
    amount: u64,
) -> ContractCall<EthSigner, ()> {
    let contract = EthSuiBridge::new(contract_address, signer.clone().into());
    let sui_recipient_address = sui_recipient_address.to_vec().into();
    let amount = U256::from(amount) * U256::exp10(18); // 1 ETH
    contract
        .bridge_eth(sui_recipient_address, sui_chain_id)
        .value(amount)
}

async fn deposit_eth_to_sui_package(
    sui_client: &SuiClient,
    sui_address: SuiAddress,
    wallet_context: &mut WalletContext,
    target_chain: u8,
    target_address: EthAddress,
    token: ObjectRef,
    bridge_object_arg: ObjectArg,
    sui_token_type_tags: &HashMap<u8, TypeTag>,
) -> SuiTransactionBlockResponse {
    let mut builder = ProgrammableTransactionBuilder::new();
    let arg_target_chain = builder.pure(target_chain).unwrap();
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
    wallet_context.execute_transaction_must_succeed(tx).await
}

async fn initiate_bridge_eth_to_sui(
    sui_bridge_client: &SuiBridgeClient,
    eth_signer: &EthSigner,
    sui_bridge_contract_address: EthAddress,
    sui_address: SuiAddress,
    eth_address: EthAddress,
    eth_chain_id: u8,
    sui_chain_id: u8,
    amount: u64,
    sui_amount: u64,
    token_id: u8,
    nonce: u64,
) {
    let eth_tx = deposit_native_eth_to_sol_contract(
        eth_signer,
        sui_bridge_contract_address,
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
    assert_eq!(eth_bridge_event.source_chain_id, eth_chain_id);
    assert_eq!(eth_bridge_event.nonce, nonce);
    assert_eq!(eth_bridge_event.destination_chain_id, sui_chain_id);
    assert_eq!(eth_bridge_event.token_id, token_id);
    assert_eq!(eth_bridge_event.sui_adjusted_amount, sui_amount);
    assert_eq!(eth_bridge_event.sender_address, eth_address);
    assert_eq!(eth_bridge_event.recipient_address, sui_address.to_vec());

    wait_for_transfer_action_status(
        sui_bridge_client,
        eth_chain_id,
        0,
        BridgeActionStatus::Claimed,
    )
    .await;
    info!("Eth to Sui bridge transfer claimed");
}

async fn initiate_bridge_sui_to_eth(
    sui_bridge_client: &SuiBridgeClient,
    sui_client: &SuiClient,
    sui_address: SuiAddress,
    wallet_context: &mut WalletContext,
    eth_chain_id: u8,
    sui_chain_id: u8,
    eth_address: EthAddress,
    token: ObjectRef,
    nonce: u64,
    bridge_object_arg: ObjectArg,
    sui_amount: u64,
    sui_token_type_tags: &HashMap<u8, TypeTag>,
) -> SuiToEthBridgeAction {
    let resp = deposit_eth_to_sui_package(
        sui_client,
        sui_address,
        wallet_context,
        eth_chain_id,
        eth_address,
        token,
        bridge_object_arg,
        sui_token_type_tags,
    )
    .await;
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

    assert_eq!(bridge_event.sui_bridge_event.nonce, nonce);
    assert_eq!(
        bridge_event.sui_bridge_event.sui_chain_id as u8,
        sui_chain_id
    );
    assert_eq!(
        bridge_event.sui_bridge_event.eth_chain_id as u8,
        eth_chain_id
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
        sui_bridge_client,
        sui_chain_id,
        nonce,
        BridgeActionStatus::Approved,
    )
    .await;
    info!("Sui to Eth bridge transfer approved");

    bridge_event
}

async fn wait_for_transfer_action_status(
    sui_bridge_client: &SuiBridgeClient,
    chain_id: u8,
    nonce: u64,
    status: BridgeActionStatus,
) {
    // Wait for the bridge action to be approved
    let now = std::time::Instant::now();
    loop {
        let res = sui_bridge_client
            .get_token_transfer_action_onchain_status_until_success(chain_id, nonce)
            .await;
        if res == status {
            break;
        }
        if now.elapsed().as_secs() > 30 {
            panic!(
                "Timeout waiting for token transfer action to be {:?}",
                status
            );
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
