// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::consistency::ConsistentIndexCursor;
use crate::context_data::db_data_provider::PgManager;
use crate::types::cursor::{JsonCursor, Page};
use async_graphql::connection::{Connection, CursorType, Edge};

use super::big_int::BigInt;
use super::move_object::MoveObject;
use super::object::ObjectLookupKey;
use super::sui_address::SuiAddress;
use super::validator_credentials::ValidatorCredentials;
use super::{address::Address, base64::Base64};
use async_graphql::*;
use sui_types::sui_system_state::sui_system_state_summary::SuiValidatorSummary as NativeSuiValidatorSummary;

#[derive(Clone, Debug)]
pub(crate) struct Validator {
    pub validator_summary: NativeSuiValidatorSummary,
    pub at_risk: Option<u64>,
    pub report_records: Option<Vec<Address>>,
    /// The checkpoint sequence number at which this was viewed at.
    pub checkpoint_viewed_at: u64,
}

type CAddr = JsonCursor<ConsistentIndexCursor>;

#[Object]
impl Validator {
    /// The validator's address.
    async fn address(&self) -> Address {
        Address {
            address: SuiAddress::from(self.validator_summary.sui_address),
            checkpoint_viewed_at: Some(self.checkpoint_viewed_at),
        }
    }

    /// Validator's set of credentials such as public keys, network addresses and others.
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

    /// Validator's set of credentials for the next epoch.
    async fn next_epoch_credentials(&self) -> Option<ValidatorCredentials> {
        let v = &self.validator_summary;
        let credentials = ValidatorCredentials {
            protocol_pub_key: v
                .next_epoch_protocol_pubkey_bytes
                .as_ref()
                .map(Base64::from),
            network_pub_key: v.next_epoch_network_pubkey_bytes.as_ref().map(Base64::from),
            worker_pub_key: v.next_epoch_worker_pubkey_bytes.as_ref().map(Base64::from),
            proof_of_possession: v.next_epoch_proof_of_possession.as_ref().map(Base64::from),
            net_address: v.next_epoch_net_address.clone(),
            p2p_address: v.next_epoch_p2p_address.clone(),
            primary_address: v.next_epoch_primary_address.clone(),
            worker_address: v.next_epoch_worker_address.clone(),
        };
        Some(credentials)
    }

    /// Validator's name.
    async fn name(&self) -> Option<String> {
        Some(self.validator_summary.name.clone())
    }

    /// Validator's description.
    async fn description(&self) -> Option<String> {
        Some(self.validator_summary.description.clone())
    }

    /// Validator's url containing their custom image.
    async fn image_url(&self) -> Option<String> {
        Some(self.validator_summary.image_url.clone())
    }

    /// Validator's homepage URL.
    async fn project_url(&self) -> Option<String> {
        Some(self.validator_summary.project_url.clone())
    }

    /// The validator's current valid `Cap` object. Validators can delegate
    /// the operation ability to another address. The address holding this `Cap` object
    /// can then update the reference gas price and tallying rule on behalf of the validator.
    async fn operation_cap(&self, ctx: &Context<'_>) -> Result<Option<MoveObject>> {
        MoveObject::query(
            ctx.data_unchecked(),
            self.operation_cap_id(),
            ObjectLookupKey::LatestAt(self.checkpoint_viewed_at),
        )
        .await
        .extend()
    }

    /// The validator's current staking pool object, used to track the amount of stake
    /// and to compound staking rewards.
    async fn staking_pool(&self, ctx: &Context<'_>) -> Result<Option<MoveObject>> {
        MoveObject::query(
            ctx.data_unchecked(),
            self.staking_pool_id(),
            ObjectLookupKey::LatestAt(self.checkpoint_viewed_at),
        )
        .await
        .extend()
    }

    /// The validator's current exchange object. The exchange rate is used to determine
    /// the amount of SUI tokens that each past SUI staker can withdraw in the future.
    async fn exchange_rates(&self, ctx: &Context<'_>) -> Result<Option<MoveObject>> {
        MoveObject::query(
            ctx.data_unchecked(),
            self.exchange_rates_id(),
            ObjectLookupKey::LatestAt(self.checkpoint_viewed_at),
        )
        .await
        .extend()
    }

    /// Number of exchange rates in the table.
    async fn exchange_rates_size(&self) -> Option<u64> {
        Some(self.validator_summary.exchange_rates_size)
    }

    /// The epoch at which this pool became active.
    async fn staking_pool_activation_epoch(&self) -> Option<u64> {
        self.validator_summary.staking_pool_activation_epoch
    }

    /// The total number of SUI tokens in this pool.
    async fn staking_pool_sui_balance(&self) -> Option<BigInt> {
        Some(BigInt::from(
            self.validator_summary.staking_pool_sui_balance,
        ))
    }

    /// The epoch stake rewards will be added here at the end of each epoch.
    async fn rewards_pool(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator_summary.rewards_pool))
    }

    /// Total number of pool tokens issued by the pool.
    async fn pool_token_balance(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator_summary.pool_token_balance))
    }

    /// Pending stake amount for this epoch.
    async fn pending_stake(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator_summary.pending_stake))
    }

    /// Pending stake withdrawn during the current epoch, emptied at epoch boundaries.
    async fn pending_total_sui_withdraw(&self) -> Option<BigInt> {
        Some(BigInt::from(
            self.validator_summary.pending_total_sui_withdraw,
        ))
    }

    /// Pending pool token withdrawn during the current epoch, emptied at epoch boundaries.
    async fn pending_pool_token_withdraw(&self) -> Option<BigInt> {
        Some(BigInt::from(
            self.validator_summary.pending_pool_token_withdraw,
        ))
    }

    /// The voting power of this validator in basis points (e.g., 100 = 1% voting power).
    async fn voting_power(&self) -> Option<u64> {
        Some(self.validator_summary.voting_power)
    }

    // TODO async fn stake_units(&self) -> Option<u64>{}

    /// The reference gas price for this epoch.
    async fn gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator_summary.gas_price))
    }

    /// The fee charged by the validator for staking services.
    async fn commission_rate(&self) -> Option<u64> {
        Some(self.validator_summary.commission_rate)
    }

    /// The total number of SUI tokens in this pool plus
    /// the pending stake amount for this epoch.
    async fn next_epoch_stake(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator_summary.next_epoch_stake))
    }

    /// The validator's gas price quote for the next epoch.
    async fn next_epoch_gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator_summary.next_epoch_gas_price))
    }

    /// The proposed next epoch fee for the validator's staking services.
    async fn next_epoch_commission_rate(&self) -> Option<u64> {
        Some(self.validator_summary.next_epoch_commission_rate)
    }

    /// The number of epochs for which this validator has been below the
    /// low stake threshold.
    async fn at_risk(&self) -> Option<u64> {
        self.at_risk
    }

    /// The addresses of other validators this validator has reported.
    async fn report_records(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        before: Option<CAddr>,
        last: Option<u64>,
        after: Option<CAddr>,
    ) -> Result<Connection<String, Address>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        let mut connection = Connection::new(false, false);
        let Some(addresses) = &self.report_records else {
            return Ok(connection);
        };

        let Some((prev, next, _, cs)) =
            page.paginate_consistent_indices(addresses.len(), self.checkpoint_viewed_at)?
        else {
            return Ok(connection);
        };

        connection.has_previous_page = prev;
        connection.has_next_page = next;

        for c in cs {
            connection.edges.push(Edge::new(
                c.encode_cursor(),
                Address {
                    address: addresses[c.ix].address,
                    checkpoint_viewed_at: Some(c.c),
                },
            ));
        }

        Ok(connection)
    }

    /// The APY of this validator in basis points.
    /// To get the APY in percentage, divide by 100.
    async fn apy(&self, ctx: &Context<'_>) -> Result<Option<u64>, Error> {
        Ok(ctx
            .data_unchecked::<PgManager>()
            .fetch_validator_apys(&self.validator_summary.sui_address)
            .await?
            .map(|x| (x * 10000.0) as u64))
    }
}

impl Validator {
    pub fn operation_cap_id(&self) -> SuiAddress {
        SuiAddress::from_array(**self.validator_summary.operation_cap_id)
    }
    pub fn staking_pool_id(&self) -> SuiAddress {
        SuiAddress::from_array(**self.validator_summary.staking_pool_id)
    }
    pub fn exchange_rates_id(&self) -> SuiAddress {
        SuiAddress::from_array(**self.validator_summary.exchange_rates_id)
    }
}
