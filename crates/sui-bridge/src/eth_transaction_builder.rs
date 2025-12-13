// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::{
    eth_bridge_committee, eth_bridge_config, eth_bridge_limiter,
    eth_committee_upgradeable_contract, eth_sui_bridge,
};
use crate::error::{BridgeError, BridgeResult};
use crate::types::{
    AddTokensOnEvmAction, AssetPriceUpdateAction, BlocklistCommitteeAction,
    BridgeCommitteeValiditySignInfo, EvmContractUpgradeAction, LimitUpdateAction,
    VerifiedCertifiedBridgeAction,
};
use crate::types::{BridgeAction, EmergencyAction};
use alloy::network::TransactionBuilder;
use alloy::primitives::{Address as EthAddress, Bytes};
use alloy::rpc::types::TransactionRequest;
use alloy::sol_types::SolCall;

pub async fn build_eth_transaction(
    contract_address: EthAddress,
    action: VerifiedCertifiedBridgeAction,
) -> BridgeResult<TransactionRequest> {
    if !action.is_governance_action() {
        return Err(BridgeError::ActionIsNotGovernanceAction(
            action.data().clone(),
        ));
    }
    // TODO: Check chain id?
    let sigs = action.auth_sig();
    match action.data() {
        BridgeAction::SuiToEthBridgeAction(_) => {
            unreachable!()
        }
        BridgeAction::SuiToEthTokenTransfer(_) => {
            unreachable!()
        }
        BridgeAction::EthToSuiBridgeAction(_) => {
            unreachable!()
        }
        BridgeAction::EmergencyAction(action) => {
            build_emergency_op_approve_transaction(contract_address, action.clone(), sigs)
        }
        BridgeAction::BlocklistCommitteeAction(action) => {
            build_committee_blocklist_approve_transaction(contract_address, action.clone(), sigs)
        }
        BridgeAction::LimitUpdateAction(action) => {
            build_limit_update_approve_transaction(contract_address, action.clone(), sigs)
        }
        BridgeAction::AssetPriceUpdateAction(action) => {
            build_asset_price_update_approve_transaction(contract_address, action.clone(), sigs)
        }
        BridgeAction::EvmContractUpgradeAction(action) => {
            build_evm_upgrade_transaction(action.clone(), sigs)
        }
        BridgeAction::AddTokensOnSuiAction(_) => {
            unreachable!();
        }
        BridgeAction::AddTokensOnEvmAction(action) => {
            build_add_tokens_on_evm_transaction(contract_address, action.clone(), sigs)
        }
    }
}

pub fn build_emergency_op_approve_transaction(
    contract_address: EthAddress,
    action: EmergencyAction,
    sigs: &BridgeCommitteeValiditySignInfo,
) -> BridgeResult<TransactionRequest> {
    let message: eth_sui_bridge::BridgeUtils::Message = action.clone().try_into()?;
    let signatures = sigs
        .signatures
        .values()
        .map(|sig| Bytes::from(sig.as_ref().to_vec()))
        .collect::<Vec<_>>();

    let call = eth_sui_bridge::EthSuiBridge::executeEmergencyOpWithSignaturesCall {
        signatures,
        message,
    };
    Ok(TransactionRequest::default()
        .with_to(contract_address)
        .with_input(call.abi_encode()))
}

pub fn build_committee_blocklist_approve_transaction(
    contract_address: EthAddress,
    action: BlocklistCommitteeAction,
    sigs: &BridgeCommitteeValiditySignInfo,
) -> BridgeResult<TransactionRequest> {
    let message: eth_bridge_committee::BridgeUtils::Message = action.clone().try_into()?;
    let signatures = sigs
        .signatures
        .values()
        .map(|sig| Bytes::from(sig.as_ref().to_vec()))
        .collect::<Vec<_>>();

    let call = eth_bridge_committee::EthBridgeCommittee::updateBlocklistWithSignaturesCall {
        signatures,
        message,
    };
    Ok(TransactionRequest::default()
        .with_to(contract_address)
        .with_input(call.abi_encode()))
}

pub fn build_limit_update_approve_transaction(
    contract_address: EthAddress,
    action: LimitUpdateAction,
    sigs: &BridgeCommitteeValiditySignInfo,
) -> BridgeResult<TransactionRequest> {
    let message: eth_bridge_limiter::BridgeUtils::Message = action.clone().try_into()?;
    let signatures = sigs
        .signatures
        .values()
        .map(|sig| Bytes::from(sig.as_ref().to_vec()))
        .collect::<Vec<_>>();

    let call = eth_bridge_limiter::EthBridgeLimiter::updateLimitWithSignaturesCall {
        signatures,
        message,
    };
    Ok(TransactionRequest::default()
        .with_to(contract_address)
        .with_input(call.abi_encode()))
}

pub fn build_asset_price_update_approve_transaction(
    contract_address: EthAddress,
    action: AssetPriceUpdateAction,
    sigs: &BridgeCommitteeValiditySignInfo,
) -> BridgeResult<TransactionRequest> {
    let message: eth_bridge_config::BridgeUtils::Message = action.clone().try_into()?;
    let signatures = sigs
        .signatures
        .values()
        .map(|sig| Bytes::from(sig.as_ref().to_vec()))
        .collect::<Vec<_>>();

    let call = eth_bridge_config::EthBridgeConfig::updateTokenPriceWithSignaturesCall {
        signatures,
        message,
    };
    Ok(TransactionRequest::default()
        .with_to(contract_address)
        .with_input(call.abi_encode()))
}

pub fn build_add_tokens_on_evm_transaction(
    contract_address: EthAddress,
    action: AddTokensOnEvmAction,
    sigs: &BridgeCommitteeValiditySignInfo,
) -> BridgeResult<TransactionRequest> {
    let message: eth_bridge_config::BridgeUtils::Message = action.clone().try_into()?;
    let signatures = sigs
        .signatures
        .values()
        .map(|sig| Bytes::from(sig.as_ref().to_vec()))
        .collect::<Vec<_>>();

    let call = eth_bridge_config::EthBridgeConfig::addTokensWithSignaturesCall {
        signatures,
        message,
    };
    Ok(TransactionRequest::default()
        .with_to(contract_address)
        .with_input(call.abi_encode()))
}

pub fn build_evm_upgrade_transaction(
    action: EvmContractUpgradeAction,
    sigs: &BridgeCommitteeValiditySignInfo,
) -> BridgeResult<TransactionRequest> {
    let contract_address = action.proxy_address;
    let message: eth_committee_upgradeable_contract::BridgeUtils::Message =
        action.clone().try_into()?;
    let signatures = sigs
        .signatures
        .values()
        .map(|sig| Bytes::from(sig.as_ref().to_vec()))
        .collect::<Vec<_>>();

    let call = eth_committee_upgradeable_contract::EthCommitteeUpgradeableContract::upgradeWithSignaturesCall {
        signatures,
        message,
    };
    Ok(TransactionRequest::default()
        .with_to(contract_address)
        .with_input(call.abi_encode()))
}

// TODO: add tests for eth transaction building
