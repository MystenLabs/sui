// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::{EthSuiBridge, eth_sui_bridge};
use crate::client::bridge_authority_aggregator::BridgeAuthorityAggregator;

use crate::e2e_tests::test_utils::{
    BridgeTestClusterBuilder, get_signatures, initiate_bridge_eth_to_sui,
    initiate_bridge_sui_to_eth, initiate_bridge_sui_to_eth_v2, send_eth_tx_and_get_tx_receipt,
};
use crate::sui_transaction_builder::build_sui_transaction;
use crate::types::{BridgeAction, EmergencyAction};
use crate::types::{BridgeActionStatus, EmergencyActionType};
use ethers::prelude::*;
use ethers::types::Address as EthAddress;
use std::sync::Arc;
use sui_json_rpc_types::SuiExecutionStatus;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_types::bridge::{BridgeChainId, TOKEN_ID_ETH};
use tracing::info;

#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
async fn test_sui_bridge_paused() {
    telemetry_subscribers::init_for_testing();

    // approve pause action in bridge nodes
    let pause_action = BridgeAction::EmergencyAction(EmergencyAction {
        nonce: 0,
        chain_id: BridgeChainId::SuiCustom,
        action_type: EmergencyActionType::Pause,
    });

    let unpause_action = BridgeAction::EmergencyAction(EmergencyAction {
        nonce: 1,
        chain_id: BridgeChainId::SuiCustom,
        action_type: EmergencyActionType::Unpause,
    });

    // Setup bridge test env
    let bridge_test_cluster = BridgeTestClusterBuilder::new()
        .with_eth_env(true)
        .with_bridge_cluster(true)
        .with_num_validators(4)
        .with_approved_governance_actions(vec![
            vec![pause_action.clone(), unpause_action.clone()],
            vec![unpause_action.clone()],
            vec![unpause_action.clone()],
            vec![],
        ])
        .build()
        .await;

    let bridge_client = bridge_test_cluster.bridge_client();
    let sui_address = bridge_test_cluster.sui_user_address();
    let sui_token_type_tags = bridge_client.get_token_id_map().await.unwrap();

    // verify bridge are not paused
    assert!(!bridge_client.get_bridge_summary().await.unwrap().is_frozen);

    // try bridge from eth and verify it works on sui
    initiate_bridge_eth_to_sui(&bridge_test_cluster, 10, 0)
        .await
        .unwrap();
    // verify Eth was transferred to Sui address
    let eth_coin_type = sui_token_type_tags.get(&TOKEN_ID_ETH).unwrap();
    let eth_coin = bridge_client
        .jsonrpc_client()
        .coin_read_api()
        .get_coins(sui_address, Some(eth_coin_type.to_string()), None, None)
        .await
        .unwrap()
        .data;
    assert_eq!(1, eth_coin.len());

    // get pause bridge signatures from committee
    let bridge_committee = Arc::new(bridge_client.get_bridge_committee().await.unwrap());
    let agg = BridgeAuthorityAggregator::new_for_testing(bridge_committee);
    let certified_action = agg
        .request_committee_signatures(pause_action)
        .await
        .unwrap();

    // execute pause bridge on sui
    let gas = bridge_test_cluster
        .wallet()
        .get_one_gas_object_owned_by_address(sui_address)
        .await
        .unwrap()
        .unwrap();

    let tx = build_sui_transaction(
        sui_address,
        &gas,
        certified_action,
        bridge_client
            .get_mutable_bridge_object_arg_must_succeed()
            .await,
        &sui_token_type_tags,
        1000,
    )
    .unwrap();

    let response = bridge_test_cluster.sign_and_execute_transaction(&tx).await;
    assert_eq!(
        response.effects.unwrap().status(),
        &SuiExecutionStatus::Success
    );
    info!("Bridge paused");

    // verify bridge paused
    assert!(bridge_client.get_bridge_summary().await.unwrap().is_frozen);

    // Transfer from eth to sui should fail on Sui
    let eth_to_sui_bridge_action = initiate_bridge_eth_to_sui(&bridge_test_cluster, 10, 1).await;
    assert!(eth_to_sui_bridge_action.is_err());
    // message should not be recorded on Sui when the bridge is paused
    let res = bridge_test_cluster
        .bridge_client()
        .get_token_transfer_action_onchain_status_until_success(
            bridge_test_cluster.eth_chain_id() as u8,
            1,
        )
        .await;
    assert_eq!(BridgeActionStatus::NotFound, res);
    // Transfer from Sui to eth should fail
    let sui_to_eth_bridge_action = initiate_bridge_sui_to_eth(
        &bridge_test_cluster,
        EthAddress::random(),
        eth_coin.first().unwrap().object_ref(),
        0,
        10,
    )
    .await;
    assert!(sui_to_eth_bridge_action.is_err())
}

/// Tests the scenario where bridge nodes and Sui framework are upgraded to V2,
/// but EVM contracts remain on V1.
///
/// Expected behavior:
/// - V1 ETH→Sui: should work (V1 deposit on EVM, V1 claim on Sui)
/// - V1 Sui→ETH: should work (V1 deposit on Sui, V1 claim on EVM)
/// - V2 Sui→ETH: deposit and approval on Sui succeed, but claiming on EVM fails
///   because V1 EVM contract doesn't have `transferBridgedTokensWithSignaturesV2`
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_v2_sui_with_v1_evm() {
    telemetry_subscribers::init_for_testing();

    let sui_chain_id = BridgeChainId::SuiCustom as u8;
    let timer = std::time::Instant::now();
    let bridge_test_cluster = BridgeTestClusterBuilder::new()
        .with_eth_env(true)
        .with_bridge_cluster(true)
        .with_num_validators(3)
        .build()
        .await;
    info!(
        "[Timer] Bridge test cluster started in {:?}",
        timer.elapsed()
    );

    // NOTE: We intentionally do NOT call upgrade_bridge_to_v2() here.
    // EVM stays on V1, while Sui framework already has V2 functions available.

    let (eth_signer, _) = bridge_test_cluster
        .get_eth_signer_and_address()
        .await
        .unwrap();
    let sui_address = bridge_test_cluster.sui_user_address();

    // === Test 1: V1 ETH→Sui should work ===
    let timer = std::time::Instant::now();
    let amount = 15;
    let sui_amount = amount * 100_000_000;

    initiate_bridge_eth_to_sui(&bridge_test_cluster, amount, 0)
        .await
        .unwrap();
    info!(
        "[Timer] V1 Eth to Sui bridge transfer finished in {:?}",
        timer.elapsed()
    );

    // Verify ETH was received on Sui
    let eth_coin = bridge_test_cluster
        .sui_client()
        .coin_read_api()
        .get_all_coins(sui_address, None, None)
        .await
        .unwrap()
        .data
        .iter()
        .find(|c| c.coin_type.contains("ETH"))
        .expect("Recipient should have received ETH coin")
        .clone();
    assert_eq!(eth_coin.balance, sui_amount);

    // === Test 2: V1 Sui→ETH should work ===
    let timer = std::time::Instant::now();
    let eth_address_1 = EthAddress::random();

    let sui_to_eth_bridge_action = initiate_bridge_sui_to_eth(
        &bridge_test_cluster,
        eth_address_1,
        eth_coin.object_ref(),
        0, // nonce
        sui_amount,
    )
    .await
    .unwrap();
    info!(
        "[Timer] V1 Sui to Eth bridge transfer approved in {:?}",
        timer.elapsed()
    );

    // Claim on EVM using V1 function
    let message: eth_sui_bridge::Message = sui_to_eth_bridge_action.try_into().unwrap();
    let signatures = get_signatures(bridge_test_cluster.bridge_client(), 0, sui_chain_id).await;
    let eth_sui_bridge = EthSuiBridge::new(
        bridge_test_cluster.contracts().sui_bridge,
        eth_signer.clone().into(),
    );
    let call = eth_sui_bridge.transfer_bridged_tokens_with_signatures(signatures, message);
    let eth_claim_tx_receipt = send_eth_tx_and_get_tx_receipt(call).await;
    assert_eq!(eth_claim_tx_receipt.status.unwrap().as_u64(), 1);
    info!(
        "[Timer] V1 Sui to Eth bridge transfer claimed in {:?}",
        timer.elapsed()
    );

    // === Test 3: V2 Sui→ETH deposit + approval should work, but EVM claim should fail ===
    // First, do another ETH→Sui to get coins for V2 test
    let timer = std::time::Instant::now();
    initiate_bridge_eth_to_sui(&bridge_test_cluster, amount, 1)
        .await
        .unwrap();
    info!(
        "[Timer] Second Eth to Sui transfer finished in {:?}",
        timer.elapsed()
    );

    let eth_coin_for_v2 = bridge_test_cluster
        .sui_client()
        .coin_read_api()
        .get_all_coins(sui_address, None, None)
        .await
        .unwrap()
        .data
        .iter()
        .find(|c| c.coin_type.contains("ETH"))
        .expect("Should have ETH coins")
        .clone();

    // Initiate V2 Sui→ETH deposit (this should work on Sui side)
    let timer = std::time::Instant::now();
    let eth_address_2 = EthAddress::random();

    let sui_to_eth_v2_action = initiate_bridge_sui_to_eth_v2(
        &bridge_test_cluster,
        eth_address_2,
        eth_coin_for_v2.object_ref(),
        1, // nonce
        sui_amount,
    )
    .await
    .unwrap();
    info!(
        "[Timer] V2 Sui to Eth bridge transfer approved in {:?} (Sui side)",
        timer.elapsed()
    );

    // Now try to claim on EVM using V2 function - this should fail because EVM is still on V1
    let message_v2: eth_sui_bridge::Message = sui_to_eth_v2_action.try_into().unwrap();
    let signatures_v2 = get_signatures(bridge_test_cluster.bridge_client(), 1, sui_chain_id).await;

    // The V1 EVM contract doesn't have transferBridgedTokensWithSignaturesV2,
    // so calling it will fail. We verify this by attempting the call.
    let call_v2 =
        eth_sui_bridge.transfer_bridged_tokens_with_signatures_v2(signatures_v2, message_v2);
    let result = call_v2.send().await;

    // The call should fail since V1 contract doesn't have this function
    assert!(
        result.is_err(),
        "V2 claim on V1 EVM contract should fail, but succeeded"
    );
    info!("V2 claim on V1 EVM correctly failed as expected");
}

/// Tests that a V1 deposit initiated before a V2 upgrade can still be claimed
/// after the upgrade completes.
///
/// This simulates the scenario where:
/// 1. User initiates a V1 ETH→Sui deposit
/// 2. Bridge upgrades to V2 while the deposit is in flight
/// 3. The V1 deposit should still be claimable on Sui (V2 is backwards compatible)
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_v1_deposit_during_v2_upgrade() {
    telemetry_subscribers::init_for_testing();

    let timer = std::time::Instant::now();
    // Start without bridge cluster - we'll start it manually to control timing
    let mut bridge_test_cluster = BridgeTestClusterBuilder::new()
        .with_eth_env(true)
        .with_bridge_cluster(false)
        .with_num_validators(3)
        .build()
        .await;
    info!(
        "[Timer] Bridge test cluster (without nodes) started in {:?}",
        timer.elapsed()
    );

    let sui_address = bridge_test_cluster.sui_user_address();
    let (eth_signer, _) = bridge_test_cluster
        .get_eth_signer_and_address()
        .await
        .unwrap();

    // === Step 1: Initiate V1 ETH→Sui deposit on EVM BEFORE bridge cluster starts ===
    let timer = std::time::Instant::now();
    let amount = 20;
    let sui_amount = amount * 100_000_000;

    // Deposit ETH to EVM contract (V1 deposit)
    let contract = EthSuiBridge::new(
        bridge_test_cluster.contracts().sui_bridge,
        eth_signer.clone().into(),
    );
    let sui_recipient = sui_address.to_vec();
    let deposit_call = contract
        .bridge_eth(sui_recipient.into(), BridgeChainId::SuiCustom as u8)
        .value(U256::from(amount) * U256::exp10(18));
    let tx_receipt = send_eth_tx_and_get_tx_receipt(deposit_call).await;
    assert_eq!(tx_receipt.status.unwrap().as_u64(), 1);
    info!(
        "[Timer] V1 ETH deposit on EVM completed in {:?}",
        timer.elapsed()
    );

    // === Step 2: Upgrade EVM to V2 BEFORE starting bridge cluster ===
    let timer = std::time::Instant::now();
    bridge_test_cluster
        .upgrade_bridge_to_v2()
        .await
        .expect("Failed to upgrade bridge to V2");
    info!("[Timer] Bridge upgraded to V2 in {:?}", timer.elapsed());

    // === Step 3: Now start the bridge cluster ===
    // The bridge nodes will see the V1 deposit event and should still process it
    let timer = std::time::Instant::now();
    // Must set governance actions for the correct number of validators (3)
    bridge_test_cluster.set_approved_governance_actions_for_next_start(vec![
        vec![],
        vec![],
        vec![],
    ]);
    bridge_test_cluster.start_bridge_cluster().await;
    bridge_test_cluster
        .wait_for_bridge_cluster_to_be_up(10)
        .await;
    info!("[Timer] Bridge cluster started in {:?}", timer.elapsed());

    // === Step 4: Wait for the V1 deposit to be claimed on Sui ===
    // Even though bridge is now V2, the V1 message should still be processable
    let timer = std::time::Instant::now();
    let now = std::time::Instant::now();
    loop {
        let res = bridge_test_cluster
            .bridge_client()
            .get_token_transfer_action_onchain_status_until_success(
                bridge_test_cluster.eth_chain_id() as u8,
                0, // nonce
            )
            .await;
        if res == BridgeActionStatus::Claimed {
            info!(
                "[Timer] V1 deposit claimed after V2 upgrade in {:?}",
                timer.elapsed()
            );
            break;
        }
        if now.elapsed().as_secs() > 120 {
            panic!(
                "Timeout waiting for V1 deposit to be claimed after V2 upgrade. Status: {:?}",
                res
            );
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    // Verify the ETH coin was received on Sui
    let eth_coin = bridge_test_cluster
        .sui_client()
        .coin_read_api()
        .get_all_coins(sui_address, None, None)
        .await
        .unwrap()
        .data
        .iter()
        .find(|c| c.coin_type.contains("ETH"))
        .expect("Recipient should have received ETH coin after V2 upgrade")
        .clone();
    assert_eq!(eth_coin.balance, sui_amount);
    info!("V1 deposit successfully claimed after V2 upgrade - backwards compatibility confirmed!");

    // === Optional: Verify V2 operations still work after upgrade ===
    // Now that we're fully on V2, test a V2 Sui→ETH transfer
    let timer = std::time::Instant::now();
    let eth_address = EthAddress::random();
    let sui_to_eth_v2_action = initiate_bridge_sui_to_eth_v2(
        &bridge_test_cluster,
        eth_address,
        eth_coin.object_ref(),
        0, // nonce for Sui→ETH direction
        sui_amount,
    )
    .await
    .unwrap();
    info!(
        "[Timer] V2 Sui to Eth transfer approved in {:?}",
        timer.elapsed()
    );

    // Claim on EVM using V2 function (should work now that EVM is upgraded)
    let timer = std::time::Instant::now();
    let message: eth_sui_bridge::Message = sui_to_eth_v2_action.try_into().unwrap();
    let signatures = get_signatures(
        bridge_test_cluster.bridge_client(),
        0,
        BridgeChainId::SuiCustom as u8,
    )
    .await;
    let eth_sui_bridge = EthSuiBridge::new(
        bridge_test_cluster.contracts().sui_bridge,
        eth_signer.clone().into(),
    );
    let call = eth_sui_bridge.transfer_bridged_tokens_with_signatures_v2(signatures, message);
    let eth_claim_tx_receipt = send_eth_tx_and_get_tx_receipt(call).await;
    assert_eq!(eth_claim_tx_receipt.status.unwrap().as_u64(), 1);
    info!(
        "[Timer] V2 Sui to Eth transfer claimed in {:?}",
        timer.elapsed()
    );
    info!("V2 operations work correctly after upgrade!");
}
