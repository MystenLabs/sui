// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// For testing, use existing RPC as data source

use crate::types::address::Address;
use crate::types::base64::Base64;
use crate::types::big_int::BigInt;
use crate::types::sui_address::SuiAddress;
use crate::types::validator::Validator;
use crate::types::validator_credentials::ValidatorCredentials;

use sui_sdk::types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use sui_sdk::types::{
    base_types::SuiAddress as NativeSuiAddress,
    sui_system_state::sui_system_state_summary::SuiValidatorSummary,
};

pub(crate) fn convert_to_validators(
    validators: Vec<SuiValidatorSummary>,
    system_state: Option<&SuiSystemStateSummary>,
) -> Vec<Validator> {
    validators
        .iter()
        .map(|v| {
            let at_risk = system_state
                .and_then(|system_state| {
                    system_state
                        .at_risk_validators
                        .iter()
                        .find(|&(address, _)| address == &v.sui_address)
                })
                .map(|&(_, value)| value);

            let report_records = system_state
                .and_then(|system_state| {
                    system_state
                        .validator_report_records
                        .iter()
                        .find(|&(address, _)| address == &v.sui_address)
                })
                .map(|(_, value)| {
                    value
                        .iter()
                        .map(|address| SuiAddress::from_array(address.to_inner()))
                        .collect::<Vec<_>>()
                });

            let credentials = ValidatorCredentials {
                protocol_pub_key: Some(Base64::from(v.protocol_pubkey_bytes.clone())),
                network_pub_key: Some(Base64::from(v.network_pubkey_bytes.clone())),
                worker_pub_key: Some(Base64::from(v.worker_pubkey_bytes.clone())),
                proof_of_possession: Some(Base64::from(v.proof_of_possession_bytes.clone())),
                net_address: Some(v.net_address.clone()),
                p2p_address: Some(v.p2p_address.clone()),
                primary_address: Some(v.primary_address.clone()),
                worker_address: Some(v.worker_address.clone()),
            };
            Validator {
                address: Address {
                    address: SuiAddress::from(v.sui_address),
                },
                next_epoch_credentials: Some(credentials.clone()),
                credentials: Some(credentials),
                name: Some(v.name.clone()),
                description: Some(v.description.clone()),
                image_url: Some(v.image_url.clone()),
                project_url: Some(v.project_url.clone()),

                operation_cap_id: SuiAddress::from_array(**v.operation_cap_id),
                staking_pool_id: SuiAddress::from_array(**v.staking_pool_id),
                exchange_rates_id: SuiAddress::from_array(**v.exchange_rates_id),
                exchange_rates_size: Some(v.exchange_rates_size),

                staking_pool_activation_epoch: v.staking_pool_activation_epoch,
                staking_pool_sui_balance: Some(BigInt::from(v.staking_pool_sui_balance)),
                rewards_pool: Some(BigInt::from(v.rewards_pool)),
                pool_token_balance: Some(BigInt::from(v.pool_token_balance)),
                pending_stake: Some(BigInt::from(v.pending_stake)),
                pending_total_sui_withdraw: Some(BigInt::from(v.pending_total_sui_withdraw)),
                pending_pool_token_withdraw: Some(BigInt::from(v.pending_pool_token_withdraw)),
                voting_power: Some(v.voting_power),
                // stake_units: todo!(),
                gas_price: Some(BigInt::from(v.gas_price)),
                commission_rate: Some(v.commission_rate),
                next_epoch_stake: Some(BigInt::from(v.next_epoch_stake)),
                next_epoch_gas_price: Some(BigInt::from(v.next_epoch_gas_price)),
                next_epoch_commission_rate: Some(v.next_epoch_commission_rate),
                at_risk,
                report_records,
                // apy: todo!(),
            }
        })
        .collect()
}

impl From<Address> for SuiAddress {
    fn from(a: Address) -> Self {
        a.address
    }
}

impl From<SuiAddress> for Address {
    fn from(a: SuiAddress) -> Self {
        Address { address: a }
    }
}

impl From<NativeSuiAddress> for SuiAddress {
    fn from(a: NativeSuiAddress) -> Self {
        SuiAddress::from_array(a.to_inner())
    }
}

impl From<SuiAddress> for NativeSuiAddress {
    fn from(a: SuiAddress) -> Self {
        NativeSuiAddress::try_from(a.as_slice()).unwrap()
    }
}

impl From<&SuiAddress> for NativeSuiAddress {
    fn from(a: &SuiAddress) -> Self {
        NativeSuiAddress::try_from(a.as_slice()).unwrap()
    }
}
