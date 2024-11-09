// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::consistency::ConsistentIndexCursor;
use crate::data::apys::calculate_apy;
use crate::data::{DataLoader, Db};
use crate::types::cursor::{JsonCursor, Page};
use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::dataloader::Loader;
use std::collections::{BTreeMap, HashMap};
use sui_indexer::apis::GovernanceReadApi;
use sui_types::committee::EpochId;
use sui_types::sui_system_state::PoolTokenExchangeRate;

use sui_types::base_types::SuiAddress as NativeSuiAddress;

use super::big_int::BigInt;
use super::move_object::MoveObject;
use super::object::Object;
use super::owner::Owner;
use super::sui_address::SuiAddress;
use super::uint53::UInt53;
use super::validator_credentials::ValidatorCredentials;
use super::{address::Address, base64::Base64};
use crate::error::Error;
use async_graphql::*;
use sui_indexer::apis::governance_api::exchange_rates;
use sui_types::sui_system_state::sui_system_state_summary::SuiValidatorSummary as NativeSuiValidatorSummary;
#[derive(Clone, Debug)]
pub(crate) struct Validator {
    pub validator_summary: NativeSuiValidatorSummary,
    pub at_risk: Option<u64>,
    pub report_records: Option<Vec<Address>>,
    /// The checkpoint sequence number at which this was viewed at.
    pub checkpoint_viewed_at: u64,
    /// The epoch at which this validator's information was requested to be viewed at.
    pub requested_for_epoch: u64,
}

type EpochStakeSubsidyStarted = u64;

/// Loads the exchange rates from the cache and return a tuple (epoch stake subsidy started, and
/// a BTreeMap holiding the exchange rates for each epoch for each validator.
///
/// It automatically filters the exchange rate table to only include data for the epochs that are
/// less than or equal to the requested epoch.
#[async_trait::async_trait]
impl Loader<u64> for Db {
    type Value = (
        EpochStakeSubsidyStarted,
        BTreeMap<NativeSuiAddress, Vec<(EpochId, PoolTokenExchangeRate)>>,
    );
    type Error = Error;

    async fn load(
        &self,
        keys: &[u64],
    ) -> Result<
        HashMap<
            u64,
            (
                EpochStakeSubsidyStarted,
                BTreeMap<NativeSuiAddress, Vec<(EpochId, PoolTokenExchangeRate)>>,
            ),
        >,
        Error,
    > {
        let latest_sui_system_state = self
            .inner
            .get_latest_sui_system_state()
            .await
            .map_err(|_| Error::Internal("Failed to fetch latest Sui system state".to_string()))?;
        let governance_api = GovernanceReadApi::new(self.inner.clone());
        let exchange_rates = exchange_rates(&governance_api, &latest_sui_system_state)
            .await
            .map_err(|e| Error::Internal(format!("Error fetching exchange rates. {e}")))?;
        let mut results = BTreeMap::new();

        // The requested epoch is the epoch for which we want to compute the APY. For the current
        // ongoing epoch we cannot compute an APY, so we compute it for epoch - 1.
        // First need to check if that requested epoch is not the current running one. If it is,
        // then subtract one as the APY cannot be computed for a running epoch.
        // If no epoch is passed in the key, then we default to the latest epoch - 1
        // for the same reasons as above.
        let epoch_to_filter_out = if let Some(epoch) = keys.first() {
            if epoch == &latest_sui_system_state.epoch {
                *epoch - 1
            } else {
                *epoch
            }
        } else {
            latest_sui_system_state.epoch - 1
        };

        // filter the exchange rates to only include data for the epochs that are less than or
        // equal to the requested epoch. This enables us to get historical exchange rates
        // accurately and pass this to the APY calculation function
        // TODO we might even filter here by the epoch at which the stake subsidy started
        // to avoid passing that to the `calculate_apy` function and doing another filter there
        for er in exchange_rates {
            results.insert(
                er.address,
                er.rates
                    .into_iter()
                    .filter(|(epoch, _)| epoch <= &epoch_to_filter_out)
                    .collect(),
            );
        }

        let requested_epoch = match keys.first() {
            Some(x) => *x,
            None => latest_sui_system_state.epoch,
        };

        let mut r = HashMap::new();
        r.insert(
            requested_epoch,
            (latest_sui_system_state.stake_subsidy_start_epoch, results),
        );

        Ok(r)
    }
}

type CAddr = JsonCursor<ConsistentIndexCursor>;

#[Object]
impl Validator {
    /// The validator's address.
    async fn address(&self) -> Address {
        Address {
            address: SuiAddress::from(self.validator_summary.sui_address),
            checkpoint_viewed_at: self.checkpoint_viewed_at,
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
            ctx,
            self.operation_cap_id(),
            Object::latest_at(self.checkpoint_viewed_at),
        )
        .await
        .extend()
    }

    /// The validator's current staking pool object, used to track the amount of stake
    /// and to compound staking rewards.
    #[graphql(
        deprecation = "The staking pool is a wrapped object. Access its fields directly on the \
        `Validator` type."
    )]
    async fn staking_pool(&self) -> Result<Option<MoveObject>> {
        Ok(None)
    }

    /// The ID of this validator's `0x3::staking_pool::StakingPool`.
    async fn staking_pool_id(&self) -> SuiAddress {
        self.validator_summary.staking_pool_id.into()
    }

    /// The validator's current exchange object. The exchange rate is used to determine
    /// the amount of SUI tokens that each past SUI staker can withdraw in the future.
    #[graphql(
        deprecation = "The exchange object is a wrapped object. Access its dynamic fields through \
        the `exchangeRatesTable` query."
    )]
    async fn exchange_rates(&self) -> Result<Option<MoveObject>> {
        Ok(None)
    }

    /// A wrapped object containing the validator's exchange rates. This is a table from epoch
    /// number to `PoolTokenExchangeRate` value. The exchange rate is used to determine the amount
    /// of SUI tokens that each past SUI staker can withdraw in the future.
    async fn exchange_rates_table(&self) -> Result<Option<Owner>> {
        Ok(Some(Owner {
            address: self.validator_summary.exchange_rates_id.into(),
            checkpoint_viewed_at: self.checkpoint_viewed_at,
            root_version: None,
        }))
    }

    /// Number of exchange rates in the table.
    async fn exchange_rates_size(&self) -> Option<UInt53> {
        Some(self.validator_summary.exchange_rates_size.into())
    }

    /// The epoch at which this pool became active.
    async fn staking_pool_activation_epoch(&self) -> Option<UInt53> {
        self.validator_summary
            .staking_pool_activation_epoch
            .map(UInt53::from)
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
    async fn at_risk(&self) -> Option<UInt53> {
        self.at_risk.map(UInt53::from)
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
                    checkpoint_viewed_at: c.c,
                },
            ));
        }

        Ok(connection)
    }

    /// The APY of this validator in basis points.
    /// To get the APY in percentage, divide by 100.
    async fn apy(&self, ctx: &Context<'_>) -> Result<Option<u64>, Error> {
        let DataLoader(loader) = ctx.data_unchecked();
        let (stake_subsidy_start_epoch, exchange_rates) = loader
            .load_one(self.requested_for_epoch)
            .await?
            .ok_or_else(|| Error::Internal("DataLoading exchange rates failed".to_string()))?;
        let rates = exchange_rates
            .get(&self.validator_summary.sui_address)
            .ok_or_else(|| {
                Error::Internal(format!(
                    "Failed to get the exchange rate for this validator address {} for requested epoch {}",
                    self.validator_summary.sui_address, self.requested_for_epoch
                ))
            })?;

        let avg_apy = Some(calculate_apy(stake_subsidy_start_epoch, rates));

        Ok(avg_apy.map(|x| (x * 10000.0) as u64))
    }
}

impl Validator {
    pub fn operation_cap_id(&self) -> SuiAddress {
        SuiAddress::from_array(**self.validator_summary.operation_cap_id)
    }
}
