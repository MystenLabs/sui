// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::eth_bridge_limiter;
use crate::abi::{eth_bridge_committee, eth_sui_bridge, EthBridgeCommittee, EthBridgeLimiter};
use crate::error::{BridgeError, BridgeResult};
use crate::types::{
    AssetPriceUpdateAction, BlocklistCommitteeAction, BridgeCommitteeValiditySignInfo,
    LimitUpdateAction, VerifiedCertifiedBridgeAction,
};
use crate::utils::EthSigner;
use crate::{
    abi::EthSuiBridge,
    types::{BridgeAction, EmergencyAction},
};
use ethers::prelude::*;
use ethers::types::Address as EthAddress;

pub async fn build_eth_transaction(
    contract_address: EthAddress,
    signer: EthSigner,
    action: VerifiedCertifiedBridgeAction,
) -> BridgeResult<ContractCall<EthSigner, ()>> {
    if !action.is_governace_action() {
        return Err(BridgeError::ActionIsNotGovernanceAction(
            action.data().clone(),
        ));
    }
    let sigs = action.auth_sig();
    match action.data() {
        BridgeAction::EmergencyAction(action) => {
            build_emergency_op_approve_transaction(contract_address, signer, action.clone(), sigs)
                .await
        }
        BridgeAction::BlocklistCommitteeAction(action) => {
            build_committee_blocklist_approve_transaction(
                contract_address,
                signer,
                action.clone(),
                sigs,
            )
            .await
        }
        BridgeAction::LimitUpdateAction(action) => {
            build_limit_update_approve_transaction(contract_address, signer, action.clone(), sigs)
                .await
        }
        BridgeAction::AssetPriceUpdateAction(action) => {
            build_asset_price_update_approve_transaction(
                contract_address,
                signer,
                action.clone(),
                sigs,
            )
            .await
        }
        _ => unreachable!(),
    }
}

pub async fn build_emergency_op_approve_transaction(
    contract_address: EthAddress,
    signer: EthSigner,
    action: EmergencyAction,
    sigs: &BridgeCommitteeValiditySignInfo,
) -> BridgeResult<ContractCall<EthSigner, ()>> {
    let contract = EthSuiBridge::new(contract_address, signer.into());

    let message: eth_sui_bridge::Message = action.clone().into();
    let signatures = sigs
        .signatures
        .values()
        .map(|sig| Bytes::from(sig.as_ref().to_vec()))
        .collect::<Vec<_>>();
    Ok(contract.execute_emergency_op_with_signatures(signatures, message))
}

pub async fn build_committee_blocklist_approve_transaction(
    contract_address: EthAddress,
    signer: EthSigner,
    action: BlocklistCommitteeAction,
    sigs: &BridgeCommitteeValiditySignInfo,
) -> BridgeResult<ContractCall<EthSigner, ()>> {
    let contract = EthBridgeCommittee::new(contract_address, signer.into());

    let message: eth_bridge_committee::Message = action.clone().into();
    let signatures = sigs
        .signatures
        .values()
        .map(|sig| Bytes::from(sig.as_ref().to_vec()))
        .collect::<Vec<_>>();
    Ok(contract.update_blocklist_with_signatures(signatures, message))
}

pub async fn build_limit_update_approve_transaction(
    contract_address: EthAddress,
    signer: EthSigner,
    action: LimitUpdateAction,
    sigs: &BridgeCommitteeValiditySignInfo,
) -> BridgeResult<ContractCall<EthSigner, ()>> {
    let contract = EthBridgeLimiter::new(contract_address, signer.into());

    let message: eth_bridge_limiter::Message = action.clone().into();
    let signatures = sigs
        .signatures
        .values()
        .map(|sig| Bytes::from(sig.as_ref().to_vec()))
        .collect::<Vec<_>>();
    Ok(contract.update_limit_with_signatures(signatures, message))
}

pub async fn build_asset_price_update_approve_transaction(
    contract_address: EthAddress,
    signer: EthSigner,
    action: AssetPriceUpdateAction,
    sigs: &BridgeCommitteeValiditySignInfo,
) -> BridgeResult<ContractCall<EthSigner, ()>> {
    let contract = EthBridgeLimiter::new(contract_address, signer.into());
    let message: eth_bridge_limiter::Message = action.clone().into();
    let signatures = sigs
        .signatures
        .values()
        .map(|sig| Bytes::from(sig.as_ref().to_vec()))
        .collect::<Vec<_>>();
    Ok(contract.update_token_price_with_signatures(signatures, message))
}

// TODO contract upgrade

// TODO: add tests for eth transaction building
