// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::traits::{KeyPair, ToFromBytes};
use move_core_types::ident_str;
use std::{collections::HashMap, str::FromStr};
use sui_types::bridge::BRIDGE_MODULE_NAME;
use sui_types::transaction::CallArg;
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{ObjectArg, TransactionData},
    TypeTag,
};
use sui_types::{Identifier, BRIDGE_PACKAGE_ID};

use crate::crypto::BridgeAuthorityKeyPair;
use crate::{
    error::{BridgeError, BridgeResult},
    types::{BridgeAction, VerifiedCertifiedBridgeAction},
};

// TODO: pass in gas price
pub fn build_sui_transaction(
    client_address: SuiAddress,
    gas_object_ref: &ObjectRef,
    action: VerifiedCertifiedBridgeAction,
    bridge_object_arg: ObjectArg,
    sui_token_type_tags: &HashMap<u8, TypeTag>,
) -> BridgeResult<TransactionData> {
    match action.data() {
        BridgeAction::EthToSuiBridgeAction(_) => build_token_bridge_approve_transaction(
            client_address,
            gas_object_ref,
            action,
            true,
            bridge_object_arg,
            sui_token_type_tags,
        ),
        BridgeAction::SuiToEthBridgeAction(_) => build_token_bridge_approve_transaction(
            client_address,
            gas_object_ref,
            action,
            false,
            bridge_object_arg,
            sui_token_type_tags,
        ),
        BridgeAction::BlocklistCommitteeAction(_) => build_committee_blocklist_approve_transaction(
            client_address,
            gas_object_ref,
            action,
            bridge_object_arg,
        ),
        BridgeAction::EmergencyAction(_) => build_emergency_op_approve_transaction(
            client_address,
            gas_object_ref,
            action,
            bridge_object_arg,
        ),
        BridgeAction::LimitUpdateAction(_) => build_limit_update_approve_transaction(
            client_address,
            gas_object_ref,
            action,
            bridge_object_arg,
        ),
        BridgeAction::AssetPriceUpdateAction(_) => build_asset_price_update_approve_transaction(
            client_address,
            gas_object_ref,
            action,
            bridge_object_arg,
        ),
        BridgeAction::EvmContractUpgradeAction(_) => {
            // It does not need a Sui tranaction to execute EVM contract upgrade
            unreachable!()
        }
    }
}

// TODO: pass in gas price
fn build_token_bridge_approve_transaction(
    client_address: SuiAddress,
    gas_object_ref: &ObjectRef,
    action: VerifiedCertifiedBridgeAction,
    claim: bool,
    bridge_object_arg: ObjectArg,
    sui_token_type_tags: &HashMap<u8, TypeTag>,
) -> BridgeResult<TransactionData> {
    let (bridge_action, sigs) = action.into_inner().into_data_and_sig();
    let mut builder = ProgrammableTransactionBuilder::new();

    let (source_chain, seq_num, sender, target_chain, target, token_type, amount) =
        match bridge_action {
            BridgeAction::SuiToEthBridgeAction(a) => {
                let bridge_event = a.sui_bridge_event;
                (
                    bridge_event.sui_chain_id,
                    bridge_event.nonce,
                    bridge_event.sui_address.to_vec(),
                    bridge_event.eth_chain_id,
                    bridge_event.eth_address.to_fixed_bytes().to_vec(),
                    bridge_event.token_id,
                    bridge_event.amount_sui_adjusted,
                )
            }
            BridgeAction::EthToSuiBridgeAction(a) => {
                let bridge_event = a.eth_bridge_event;
                (
                    bridge_event.eth_chain_id,
                    bridge_event.nonce,
                    bridge_event.eth_address.to_fixed_bytes().to_vec(),
                    bridge_event.sui_chain_id,
                    bridge_event.sui_address.to_vec(),
                    bridge_event.token_id,
                    bridge_event.sui_adjusted_amount,
                )
            }
            _ => unreachable!(),
        };

    let source_chain = builder.pure(source_chain as u8).unwrap();
    let seq_num = builder.pure(seq_num).unwrap();
    let sender = builder.pure(sender.clone()).map_err(|e| {
        BridgeError::BridgeSerializationError(format!(
            "Failed to serialize sender: {:?}. Err: {:?}",
            sender, e
        ))
    })?;
    let target_chain = builder.pure(target_chain as u8).unwrap();
    let target = builder.pure(target.clone()).map_err(|e| {
        BridgeError::BridgeSerializationError(format!(
            "Failed to serialize target: {:?}. Err: {:?}",
            target, e
        ))
    })?;
    let arg_token_type = builder.pure(token_type).unwrap();
    let amount = builder.pure(amount).unwrap();

    let arg_msg = builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("message").to_owned(),
        ident_str!("create_token_bridge_message").to_owned(),
        vec![],
        vec![
            source_chain,
            seq_num,
            sender,
            target_chain,
            target,
            arg_token_type,
            amount,
        ],
    );

    // Unwrap: these should not fail
    let arg_bridge = builder.obj(bridge_object_arg).unwrap();
    let arg_clock = builder.input(CallArg::CLOCK_IMM).unwrap();

    let mut sig_bytes = vec![];
    for (_, sig) in sigs.signatures {
        sig_bytes.push(sig.as_bytes().to_vec());
    }
    let arg_signatures = builder.pure(sig_bytes.clone()).map_err(|e| {
        BridgeError::BridgeSerializationError(format!(
            "Failed to serialize signatures: {:?}. Err: {:?}",
            sig_bytes, e
        ))
    })?;

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        sui_types::bridge::BRIDGE_MODULE_NAME.to_owned(),
        ident_str!("approve_token_transfer").to_owned(),
        vec![],
        vec![arg_bridge, arg_msg, arg_signatures],
    );

    if claim {
        builder.programmable_move_call(
            BRIDGE_PACKAGE_ID,
            sui_types::bridge::BRIDGE_MODULE_NAME.to_owned(),
            ident_str!("claim_and_transfer_token").to_owned(),
            vec![sui_token_type_tags
                .get(&token_type)
                .ok_or(BridgeError::UnknownTokenId(token_type))?
                .clone()],
            vec![arg_bridge, arg_clock, source_chain, seq_num],
        );
    }

    let pt = builder.finish();

    Ok(TransactionData::new_programmable(
        client_address,
        vec![*gas_object_ref],
        pt,
        100_000_000,
        // TODO: use reference gas price
        1500,
    ))
}

// TODO: pass in gas price
fn build_emergency_op_approve_transaction(
    client_address: SuiAddress,
    gas_object_ref: &ObjectRef,
    action: VerifiedCertifiedBridgeAction,
    bridge_object_arg: ObjectArg,
) -> BridgeResult<TransactionData> {
    let (bridge_action, sigs) = action.into_inner().into_data_and_sig();

    let mut builder = ProgrammableTransactionBuilder::new();

    let (source_chain, seq_num, action_type) = match bridge_action {
        BridgeAction::EmergencyAction(a) => (a.chain_id, a.nonce, a.action_type),
        _ => unreachable!(),
    };

    // Unwrap: these should not fail
    let source_chain = builder.pure(source_chain as u8).unwrap();
    let seq_num = builder.pure(seq_num).unwrap();
    let action_type = builder.pure(action_type as u8).unwrap();
    let arg_bridge = builder.obj(bridge_object_arg).unwrap();

    let arg_msg = builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("message").to_owned(),
        ident_str!("create_emergency_op_message").to_owned(),
        vec![],
        vec![source_chain, seq_num, action_type],
    );

    let mut sig_bytes = vec![];
    for (_, sig) in sigs.signatures {
        sig_bytes.push(sig.as_bytes().to_vec());
    }
    let arg_signatures = builder.pure(sig_bytes.clone()).map_err(|e| {
        BridgeError::BridgeSerializationError(format!(
            "Failed to serialize signatures: {:?}. Err: {:?}",
            sig_bytes, e
        ))
    })?;

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("bridge").to_owned(),
        ident_str!("execute_system_message").to_owned(),
        vec![],
        vec![arg_bridge, arg_msg, arg_signatures],
    );

    let pt = builder.finish();

    Ok(TransactionData::new_programmable(
        client_address,
        vec![*gas_object_ref],
        pt,
        100_000_000,
        // TODO: use reference gas price
        1500,
    ))
}

// TODO: pass in gas price
fn build_committee_blocklist_approve_transaction(
    client_address: SuiAddress,
    gas_object_ref: &ObjectRef,
    action: VerifiedCertifiedBridgeAction,
    bridge_object_arg: ObjectArg,
) -> BridgeResult<TransactionData> {
    let (bridge_action, sigs) = action.into_inner().into_data_and_sig();

    let mut builder = ProgrammableTransactionBuilder::new();

    let (source_chain, seq_num, blocklist_type, blocklisted_members) = match bridge_action {
        BridgeAction::BlocklistCommitteeAction(a) => {
            (a.chain_id, a.nonce, a.blocklist_type, a.blocklisted_members)
        }
        _ => unreachable!(),
    };

    // Unwrap: these should not fail
    let source_chain = builder.pure(source_chain as u8).unwrap();
    let seq_num = builder.pure(seq_num).unwrap();
    let blocklist_type = builder.pure(blocklist_type as u8).unwrap();
    let blocklisted_members = blocklisted_members
        .into_iter()
        .map(|m| m.to_eth_address().as_bytes().to_vec())
        .collect::<Vec<_>>();
    let blocklisted_members = builder.pure(blocklisted_members).unwrap();
    let arg_bridge = builder.obj(bridge_object_arg).unwrap();

    let arg_msg = builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("message").to_owned(),
        ident_str!("create_blocklist_message").to_owned(),
        vec![],
        vec![source_chain, seq_num, blocklist_type, blocklisted_members],
    );

    let mut sig_bytes = vec![];
    for (_, sig) in sigs.signatures {
        sig_bytes.push(sig.as_bytes().to_vec());
    }
    let arg_signatures = builder.pure(sig_bytes.clone()).map_err(|e| {
        BridgeError::BridgeSerializationError(format!(
            "Failed to serialize signatures: {:?}. Err: {:?}",
            sig_bytes, e
        ))
    })?;

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("bridge").to_owned(),
        ident_str!("execute_system_message").to_owned(),
        vec![],
        vec![arg_bridge, arg_msg, arg_signatures],
    );

    let pt = builder.finish();

    Ok(TransactionData::new_programmable(
        client_address,
        vec![*gas_object_ref],
        pt,
        100_000_000,
        // TODO: use reference gas price
        1500,
    ))
}

// TODO: pass in gas price
fn build_limit_update_approve_transaction(
    client_address: SuiAddress,
    gas_object_ref: &ObjectRef,
    action: VerifiedCertifiedBridgeAction,
    bridge_object_arg: ObjectArg,
) -> BridgeResult<TransactionData> {
    let (bridge_action, sigs) = action.into_inner().into_data_and_sig();

    let mut builder = ProgrammableTransactionBuilder::new();

    let (receiving_chain_id, seq_num, sending_chain_id, new_usd_limit) = match bridge_action {
        BridgeAction::LimitUpdateAction(a) => {
            (a.chain_id, a.nonce, a.sending_chain_id, a.new_usd_limit)
        }
        _ => unreachable!(),
    };

    // Unwrap: these should not fail
    let receiving_chain_id = builder.pure(receiving_chain_id as u8).unwrap();
    let seq_num = builder.pure(seq_num).unwrap();
    let sending_chain_id = builder.pure(sending_chain_id as u8).unwrap();
    let new_usd_limit = builder.pure(new_usd_limit).unwrap();
    let arg_bridge = builder.obj(bridge_object_arg).unwrap();

    let arg_msg = builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("message").to_owned(),
        ident_str!("create_update_bridge_limit_message").to_owned(),
        vec![],
        vec![receiving_chain_id, seq_num, sending_chain_id, new_usd_limit],
    );

    let mut sig_bytes = vec![];
    for (_, sig) in sigs.signatures {
        sig_bytes.push(sig.as_bytes().to_vec());
    }
    let arg_signatures = builder.pure(sig_bytes.clone()).map_err(|e| {
        BridgeError::BridgeSerializationError(format!(
            "Failed to serialize signatures: {:?}. Err: {:?}",
            sig_bytes, e
        ))
    })?;

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("bridge").to_owned(),
        ident_str!("execute_system_message").to_owned(),
        vec![],
        vec![arg_bridge, arg_msg, arg_signatures],
    );

    let pt = builder.finish();

    Ok(TransactionData::new_programmable(
        client_address,
        vec![*gas_object_ref],
        pt,
        100_000_000,
        // TODO: use reference gas price
        1500,
    ))
}

// TODO: pass in gas price
fn build_asset_price_update_approve_transaction(
    client_address: SuiAddress,
    gas_object_ref: &ObjectRef,
    action: VerifiedCertifiedBridgeAction,
    bridge_object_arg: ObjectArg,
) -> BridgeResult<TransactionData> {
    let (bridge_action, sigs) = action.into_inner().into_data_and_sig();

    let mut builder = ProgrammableTransactionBuilder::new();

    let (source_chain, seq_num, token_id, new_usd_price) = match bridge_action {
        BridgeAction::AssetPriceUpdateAction(a) => {
            (a.chain_id, a.nonce, a.token_id, a.new_usd_price)
        }
        _ => unreachable!(),
    };

    // Unwrap: these should not fail
    let source_chain = builder.pure(source_chain as u8).unwrap();
    let token_id = builder.pure(token_id).unwrap();
    let seq_num = builder.pure(seq_num).unwrap();
    let new_price = builder.pure(new_usd_price).unwrap();
    let arg_bridge = builder.obj(bridge_object_arg).unwrap();

    let arg_msg = builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("message").to_owned(),
        ident_str!("create_update_asset_price_message").to_owned(),
        vec![],
        vec![token_id, source_chain, seq_num, new_price],
    );

    let mut sig_bytes = vec![];
    for (_, sig) in sigs.signatures {
        sig_bytes.push(sig.as_bytes().to_vec());
    }
    let arg_signatures = builder.pure(sig_bytes.clone()).map_err(|e| {
        BridgeError::BridgeSerializationError(format!(
            "Failed to serialize signatures: {:?}. Err: {:?}",
            sig_bytes, e
        ))
    })?;

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("bridge").to_owned(),
        ident_str!("execute_system_message").to_owned(),
        vec![],
        vec![arg_bridge, arg_msg, arg_signatures],
    );

    let pt = builder.finish();

    Ok(TransactionData::new_programmable(
        client_address,
        vec![*gas_object_ref],
        pt,
        100_000_000,
        // TODO: use reference gas price
        1500,
    ))
}

pub fn build_committee_register_transaction(
    validator_address: SuiAddress,
    gas_object_ref: &ObjectRef,
    bridge_object_arg: ObjectArg,
    bridge_key: BridgeAuthorityKeyPair,
    bridge_url: &str,
    ref_gas_price: u64,
) -> BridgeResult<TransactionData> {
    let mut builder = ProgrammableTransactionBuilder::new();
    let system_state = builder.obj(ObjectArg::SUI_SYSTEM_MUT).unwrap();
    let bridge = builder.obj(bridge_object_arg).unwrap();
    let pub_key = bridge_key.public().as_bytes().to_vec();
    let bridge_pubkey = builder
        .input(CallArg::Pure(bcs::to_bytes(&pub_key).unwrap()))
        .unwrap();
    let url = builder
        .input(CallArg::Pure(bcs::to_bytes(bridge_url.as_bytes()).unwrap()))
        .unwrap();
    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        BRIDGE_MODULE_NAME.into(),
        Identifier::from_str("committee_registration").unwrap(),
        vec![],
        vec![bridge, system_state, bridge_pubkey, url],
    );
    let data = TransactionData::new_programmable(
        validator_address,
        vec![*gas_object_ref],
        builder.finish(),
        1000000000,
        ref_gas_price,
    );
    Ok(data)
}

#[cfg(test)]
mod tests {
    use crate::sui_client::SuiClient;
    use crate::types::BridgeAction;
    use crate::types::EmergencyAction;
    use crate::types::EmergencyActionType;
    use crate::types::*;
    use crate::{
        crypto::BridgeAuthorityPublicKeyBytes,
        test_utils::{
            approve_action_with_validator_secrets, bridge_token, get_test_eth_to_sui_bridge_action,
            get_test_sui_to_eth_bridge_action,
        },
        BRIDGE_ENABLE_PROTOCOL_VERSION,
    };
    use ethers::types::Address as EthAddress;
    use std::collections::HashMap;
    use sui_types::bridge::{BridgeChainId, TOKEN_ID_BTC, TOKEN_ID_USDC};
    use sui_types::crypto::ToFromBytes;
    use test_cluster::TestClusterBuilder;

    #[tokio::test]
    async fn test_build_sui_transaction_for_token_transfer() {
        telemetry_subscribers::init_for_testing();
        let mut test_cluster: test_cluster::TestCluster = TestClusterBuilder::new()
            .with_protocol_version(BRIDGE_ENABLE_PROTOCOL_VERSION.into())
            .with_epoch_duration_ms(2000)
            .build_with_bridge(true)
            .await;

        let sui_client = SuiClient::new(&test_cluster.fullnode_handle.rpc_url)
            .await
            .unwrap();
        let bridge_authority_keys = test_cluster.bridge_authority_keys.take().unwrap();

        // Note: We don't call `sui_client.get_bridge_committee` here because it will err if the committee
        // is not initialized during the construction of `BridgeCommittee`.
        let committee = sui_client.get_bridge_summary().await.unwrap().committee;
        if committee.members.is_empty() {
            test_cluster.wait_for_epoch(None).await;
        }
        let context = &mut test_cluster.wallet;
        let sender = context.active_address().unwrap();
        let usdc_amount = 5000000;
        let bridge_object_arg = sui_client.get_mutable_bridge_object_arg().await.unwrap();
        let id_token_map = sui_client.get_token_id_map().await.unwrap();

        // 1. Test Eth -> Sui Transfer approval
        let action = get_test_eth_to_sui_bridge_action(None, Some(usdc_amount), Some(sender));
        // `approve_action_with_validator_secrets` covers transaction building
        let usdc_object_ref = approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            Some(sender),
            &id_token_map,
        )
        .await
        .unwrap();

        // 2. Test Sui -> Eth Transfer approval
        let bridge_event = bridge_token(
            context,
            EthAddress::random(),
            usdc_object_ref,
            id_token_map.get(&TOKEN_ID_USDC).unwrap().clone(),
            bridge_object_arg,
        )
        .await;

        let action = get_test_sui_to_eth_bridge_action(
            None,
            None,
            Some(bridge_event.nonce),
            Some(bridge_event.amount_sui_adjusted),
            Some(bridge_event.sui_address),
            Some(bridge_event.eth_address),
            Some(TOKEN_ID_USDC),
        );
        // `approve_action_with_validator_secrets` covers transaction building
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;
    }

    #[tokio::test]
    async fn test_build_sui_transaction_for_emergency_op() {
        telemetry_subscribers::init_for_testing();
        let mut test_cluster: test_cluster::TestCluster = TestClusterBuilder::new()
            .with_protocol_version((BRIDGE_ENABLE_PROTOCOL_VERSION).into())
            .with_epoch_duration_ms(15000)
            .build_with_bridge(true)
            .await;
        let sui_client = SuiClient::new(&test_cluster.fullnode_handle.rpc_url)
            .await
            .unwrap();
        let bridge_authority_keys = test_cluster.bridge_authority_keys.take().unwrap();

        // Note: We don't call `sui_client.get_bridge_committee` here because it will err if the committee
        // is not initialized during the construction of `BridgeCommittee`.
        let committee = sui_client.get_bridge_summary().await.unwrap().committee;
        if committee.members.is_empty() {
            test_cluster.wait_for_epoch(None).await;
        }
        let summary = sui_client.get_bridge_summary().await.unwrap();
        assert!(!summary.is_frozen);

        let context = &mut test_cluster.wallet;
        let bridge_object_arg = sui_client.get_mutable_bridge_object_arg().await.unwrap();
        let id_token_map = sui_client.get_token_id_map().await.unwrap();

        // 1. Pause
        let action = BridgeAction::EmergencyAction(EmergencyAction {
            nonce: 0,
            chain_id: BridgeChainId::SuiLocalTest,
            action_type: EmergencyActionType::Pause,
        });
        // `approve_action_with_validator_secrets` covers transaction building
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;
        let summary = sui_client.get_bridge_summary().await.unwrap();
        assert!(summary.is_frozen);

        // 2. Unpause
        let action = BridgeAction::EmergencyAction(EmergencyAction {
            nonce: 1,
            chain_id: BridgeChainId::SuiLocalTest,
            action_type: EmergencyActionType::Unpause,
        });
        // `approve_action_with_validator_secrets` covers transaction building
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;
        let summary = sui_client.get_bridge_summary().await.unwrap();
        assert!(!summary.is_frozen);
    }

    #[tokio::test]
    async fn test_build_sui_transaction_for_committee_blocklist() {
        telemetry_subscribers::init_for_testing();
        let mut test_cluster: test_cluster::TestCluster = TestClusterBuilder::new()
            .with_protocol_version((BRIDGE_ENABLE_PROTOCOL_VERSION).into())
            .with_epoch_duration_ms(15000)
            .build_with_bridge(true)
            .await;
        let sui_client = SuiClient::new(&test_cluster.fullnode_handle.rpc_url)
            .await
            .unwrap();
        let bridge_authority_keys = test_cluster.bridge_authority_keys.take().unwrap();

        // Note: We don't call `sui_client.get_bridge_committee` here because it will err if the committee
        // is not initialized during the construction of `BridgeCommittee`.
        let committee = sui_client.get_bridge_summary().await.unwrap().committee;
        if committee.members.is_empty() {
            test_cluster.wait_for_epoch(None).await;
        }
        let committee = sui_client.get_bridge_summary().await.unwrap().committee;
        let victim = committee.members.first().unwrap().clone().1;
        for member in committee.members {
            assert!(!member.1.blocklisted);
        }

        let context = &mut test_cluster.wallet;
        let bridge_object_arg = sui_client.get_mutable_bridge_object_arg().await.unwrap();
        let id_token_map = sui_client.get_token_id_map().await.unwrap();

        // 1. blocklist The victim
        let action = BridgeAction::BlocklistCommitteeAction(BlocklistCommitteeAction {
            nonce: 0,
            chain_id: BridgeChainId::SuiLocalTest,
            blocklist_type: BlocklistType::Blocklist,
            blocklisted_members: vec![BridgeAuthorityPublicKeyBytes::from_bytes(
                &victim.bridge_pubkey_bytes,
            )
            .unwrap()],
        });
        // `approve_action_with_validator_secrets` covers transaction building
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;
        let committee = sui_client.get_bridge_summary().await.unwrap().committee;
        for member in committee.members {
            if member.1.bridge_pubkey_bytes == victim.bridge_pubkey_bytes {
                assert!(member.1.blocklisted);
            } else {
                assert!(!member.1.blocklisted);
            }
        }

        // 2. unblocklist the victim
        let action = BridgeAction::BlocklistCommitteeAction(BlocklistCommitteeAction {
            nonce: 1,
            chain_id: BridgeChainId::SuiLocalTest,
            blocklist_type: BlocklistType::Unblocklist,
            blocklisted_members: vec![BridgeAuthorityPublicKeyBytes::from_bytes(
                &victim.bridge_pubkey_bytes,
            )
            .unwrap()],
        });
        // `approve_action_with_validator_secrets` covers transaction building
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;
        let committee = sui_client.get_bridge_summary().await.unwrap().committee;
        for member in committee.members {
            assert!(!member.1.blocklisted);
        }
    }

    #[tokio::test]
    async fn test_build_sui_transaction_for_limit_update() {
        telemetry_subscribers::init_for_testing();
        let mut test_cluster: test_cluster::TestCluster = TestClusterBuilder::new()
            .with_protocol_version((BRIDGE_ENABLE_PROTOCOL_VERSION).into())
            .with_epoch_duration_ms(15000)
            .build_with_bridge(true)
            .await;
        let sui_client = SuiClient::new(&test_cluster.fullnode_handle.rpc_url)
            .await
            .unwrap();
        let bridge_authority_keys = test_cluster.bridge_authority_keys.take().unwrap();

        // Note: We don't call `sui_client.get_bridge_committee` here because it will err if the committee
        // is not initialized during the construction of `BridgeCommittee`.
        let committee = sui_client.get_bridge_summary().await.unwrap().committee;
        if committee.members.is_empty() {
            test_cluster.wait_for_epoch(None).await;
        }
        let transfer_limit = sui_client
            .get_bridge_summary()
            .await
            .unwrap()
            .limiter
            .transfer_limit
            .into_iter()
            .map(|(s, d, l)| ((s, d), l))
            .collect::<HashMap<_, _>>();

        let context = &mut test_cluster.wallet;
        let bridge_object_arg = sui_client.get_mutable_bridge_object_arg().await.unwrap();
        let id_token_map = sui_client.get_token_id_map().await.unwrap();

        // update limit
        let action = BridgeAction::LimitUpdateAction(LimitUpdateAction {
            nonce: 0,
            chain_id: BridgeChainId::SuiLocalTest,
            sending_chain_id: BridgeChainId::EthLocalTest,
            new_usd_limit: 6_666_666 * USD_MULTIPLIER, // $1M USD
        });
        // `approve_action_with_validator_secrets` covers transaction building
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;
        let new_transfer_limit = sui_client
            .get_bridge_summary()
            .await
            .unwrap()
            .limiter
            .transfer_limit;
        for limit in new_transfer_limit {
            if limit.0 == BridgeChainId::EthLocalTest && limit.1 == BridgeChainId::SuiLocalTest {
                assert_eq!(limit.2, 6_666_666 * USD_MULTIPLIER);
            } else {
                assert_eq!(limit.2, *transfer_limit.get(&(limit.0, limit.1)).unwrap());
            }
        }
    }

    #[tokio::test]
    async fn test_build_sui_transaction_for_price_update() {
        telemetry_subscribers::init_for_testing();
        let mut test_cluster: test_cluster::TestCluster = TestClusterBuilder::new()
            .with_protocol_version(BRIDGE_ENABLE_PROTOCOL_VERSION.into())
            .with_epoch_duration_ms(15000)
            .build_with_bridge(true)
            .await;
        let sui_client = SuiClient::new(&test_cluster.fullnode_handle.rpc_url)
            .await
            .unwrap();
        let bridge_authority_keys = test_cluster.bridge_authority_keys.take().unwrap();

        // Note: We don't call `sui_client.get_bridge_committee` here because it will err if the committee
        // is not initialized during the construction of `BridgeCommittee`.
        let committee = sui_client.get_bridge_summary().await.unwrap().committee;
        if committee.members.is_empty() {
            test_cluster.wait_for_epoch(None).await;
        }
        let notional_values = sui_client.get_notional_values().await.unwrap();
        assert_ne!(notional_values[&TOKEN_ID_USDC], 69_000 * USD_MULTIPLIER);

        let context = &mut test_cluster.wallet;
        let bridge_object_arg = sui_client.get_mutable_bridge_object_arg().await.unwrap();
        let id_token_map = sui_client.get_token_id_map().await.unwrap();

        // update price
        let action = BridgeAction::AssetPriceUpdateAction(AssetPriceUpdateAction {
            nonce: 0,
            chain_id: BridgeChainId::SuiLocalTest,
            token_id: TOKEN_ID_BTC,
            new_usd_price: 69_000 * USD_MULTIPLIER, // $69k USD
        });
        // `approve_action_with_validator_secrets` covers transaction building
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;
        let new_notional_values = sui_client.get_notional_values().await.unwrap();
        for (token_id, price) in new_notional_values {
            if token_id == TOKEN_ID_BTC {
                assert_eq!(price, 69_000 * USD_MULTIPLIER);
            } else {
                assert_eq!(price, *notional_values.get(&token_id).unwrap());
            }
        }
    }
}
