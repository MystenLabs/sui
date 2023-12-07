// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::db_data_provider::PgManager;

use super::big_int::BigInt;
use super::move_object::MoveObject;
use super::sui_address::SuiAddress;
use super::validator_credentials::ValidatorCredentials;
use super::{address::Address, base64::Base64};
use async_graphql::*;

use sui_types::sui_system_state::sui_system_state_summary::{
    SuiSystemStateSummary as NativeSuiSystemStateSummary,
    SuiValidatorSummary as NativeSuiValidatorSummary,
};
#[derive(Clone, Debug)]
pub(crate) struct Validator {
    pub validator_summary: NativeSuiValidatorSummary,
    pub system_state_summary: Option<NativeSuiSystemStateSummary>,
}

#[Object]
impl Validator {
    async fn address(&self) -> Address {
        Address {
            address: SuiAddress::from(self.validator_summary.sui_address),
        }
    }
    async fn credentials(&self) -> Option<ValidatorCredentials> {
        let v = &self.validator_summary;
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
        Some(credentials)
    }
    async fn next_epoch_credentials(&self) -> Option<ValidatorCredentials> {
        let v = &self.validator_summary;
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
        Some(credentials)
    }
    async fn name(&self) -> Option<String> {
        Some(self.validator_summary.name.clone())
    }
    async fn description(&self) -> Option<String> {
        Some(self.validator_summary.description.clone())
    }
    async fn image_url(&self) -> Option<String> {
        Some(self.validator_summary.image_url.clone())
    }
    async fn project_url(&self) -> Option<String> {
        Some(self.validator_summary.project_url.clone())
    }
    #[graphql(skip)]
    async fn operation_cap_id(&self) -> SuiAddress {
        SuiAddress::from_array(**self.validator_summary.operation_cap_id)
    }
    #[graphql(skip)]
    async fn staking_pool_id(&self) -> SuiAddress {
        SuiAddress::from_array(**self.validator_summary.staking_pool_id)
    }
    #[graphql(skip)]
    async fn exchange_rates_id(&self) -> SuiAddress {
        SuiAddress::from_array(**self.validator_summary.exchange_rates_id)
    }
    async fn exchange_rates_size(&self) -> Option<u64> {
        Some(self.validator_summary.exchange_rates_size)
    }
    async fn staking_pool_activation_epoch(&self) -> Option<u64> {
        self.validator_summary.staking_pool_activation_epoch
    }
    async fn staking_pool_sui_balance(&self) -> Option<BigInt> {
        Some(BigInt::from(
            self.validator_summary.staking_pool_sui_balance,
        ))
    }
    async fn rewards_pool(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator_summary.rewards_pool))
    }
    async fn pool_token_balance(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator_summary.pool_token_balance))
    }
    async fn pending_stake(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator_summary.pending_stake))
    }
    async fn pending_total_sui_withdraw(&self) -> Option<BigInt> {
        Some(BigInt::from(
            self.validator_summary.pending_total_sui_withdraw,
        ))
    }
    async fn pending_pool_token_withdraw(&self) -> Option<BigInt> {
        Some(BigInt::from(
            self.validator_summary.pending_pool_token_withdraw,
        ))
    }
    async fn voting_power(&self) -> Option<u64> {
        Some(self.validator_summary.voting_power)
    }
    // async fn stake_units(&self) -> Option<u64>{}
    async fn gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator_summary.gas_price))
    }
    async fn commission_rate(&self) -> Option<u64> {
        Some(self.validator_summary.commission_rate)
    }
    async fn next_epoch_stake(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator_summary.next_epoch_stake))
    }
    async fn next_epoch_gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator_summary.next_epoch_gas_price))
    }
    async fn next_epoch_commission_rate(&self) -> Option<u64> {
        Some(self.validator_summary.next_epoch_commission_rate)
    }
    async fn at_risk(&self) -> Option<u64> {
        self.system_state_summary
            .as_ref()
            .and_then(|system_state| {
                system_state
                    .at_risk_validators
                    .iter()
                    .find(|&(address, _)| address == &self.validator_summary.sui_address)
            })
            .map(|&(_, value)| value.clone())
    } // only available on sui_system_state_summary
    async fn report_records(&self) -> Option<Vec<SuiAddress>> {
        self.system_state_summary
            .as_ref()
            .and_then(|system_state| {
                system_state
                    .validator_report_records
                    .iter()
                    .find(|&(address, _)| address == &self.validator_summary.sui_address)
            })
            .map(|(_, value)| {
                value
                    .iter()
                    .map(|address| SuiAddress::from_array(address.to_inner()))
                    .collect::<Vec<_>>()
            })
    }
    // async fn apy(&self) -> Option<u64>{}
    async fn operation_cap(&self, ctx: &Context<'_>) -> Result<Option<MoveObject>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_move_obj(self.operation_cap_id().await, None)
            .await
            .extend()
    }

    async fn staking_pool(&self, ctx: &Context<'_>) -> Result<Option<MoveObject>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_move_obj(self.staking_pool_id().await, None)
            .await
            .extend()
    }

    async fn exchange_rates(&self, ctx: &Context<'_>) -> Result<Option<MoveObject>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_move_obj(self.exchange_rates_id().await, None)
            .await
            .extend()
    }
}
