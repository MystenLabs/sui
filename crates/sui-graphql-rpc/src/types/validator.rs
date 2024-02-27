// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::consistency::ConsistentIndexCursor;
use crate::data::{Db, DbConnection, QueryExecutor};
use crate::types::cursor::{JsonCursor, Page};
use async_graphql::connection::{Connection, CursorType, Edge};
use diesel::{ExpressionMethods, QueryDsl};
use sui_indexer::models::objects::StoredObject;
use sui_indexer::schema::objects;
use sui_indexer::types::OwnerType;
use sui_types::committee::EpochId;
use sui_types::sui_system_state::PoolTokenExchangeRate;

use super::big_int::BigInt;
use super::move_object::MoveObject;
use super::object::ObjectLookupKey;
use super::sui_address::SuiAddress;
use super::validator_credentials::ValidatorCredentials;
use super::{address::Address, base64::Base64};
use crate::error::Error;
use async_graphql::*;
use itertools::Itertools;
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
    async fn apy(&self, ctx: &Context<'_>) -> Result<Option<u64>> {
        let db = ctx.data_unchecked::<Db>();
        let mut rates = vec![];
        let stake_subsidy_start_epoch = 20; // TODO: (wlmyng) how can we avoid hardcoding this
        let exchange_rates_id = self.validator_summary.exchange_rates_id.to_vec();
        let exchange_rates_size = self.validator_summary.exchange_rates_size as i64;

        // Query for exchange rates dynamic fields, bounded by `exchange_rates_size`. We don't need
        // to order by `object_id` as we will sort by `epoch` later. We can try optimizing this
        // query by grabbing the last 30 records before a certain checkpoint, but for the query to
        // be correct, we would need to sort by `checkpoint_sequence_number DESC`. This results in
        // about the same cost as the current query below.
        let dynamic_fields: Vec<StoredObject> = db
            .execute(move |conn| {
                conn.results(move || {
                    objects::dsl::objects
                        .filter(objects::dsl::owner_type.eq(OwnerType::Object as i16))
                        .filter(objects::dsl::owner_id.eq(exchange_rates_id.clone()))
                        .limit(exchange_rates_size)
                })
            })
            .await?;

        for df in dynamic_fields {
            // Operate only on `PoolTokenExchangeRate` dynamic fields.
            let dynamic_field = df.to_dynamic_field::<EpochId, PoolTokenExchangeRate>();

            if let Some(dynamic_field) = dynamic_field {
                rates.push((dynamic_field.name, dynamic_field.value));
            }
        }

        rates.sort_by(|(a, _), (b, _)| a.cmp(b).reverse());

        let exchange_rates = rates.into_iter().filter_map(|(epoch, rate)| {
            if epoch >= stake_subsidy_start_epoch {
                Some(rate)
            } else {
                None
            }
        });

        let average_apy = if exchange_rates.clone().count() >= 2 {
            // rates are sorted by epoch in descending order.
            let er_e = exchange_rates.clone().dropping(1);
            // rate e+1
            let er_e_1 = exchange_rates.dropping_back(1);
            let apys = er_e
                .zip(er_e_1)
                .map(calculate_apy)
                .filter(|apy| *apy > 0.0 && *apy < 0.1)
                .take(30)
                .collect::<Vec<_>>();

            let apy_counts = apys.len() as f64;
            apys.iter().sum::<f64>() / apy_counts
        } else {
            0.0
        };

        Ok(Some((average_apy * 10000.0) as u64))
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

// APY_e = (ER_e+1 / ER_e) ^ 365
fn calculate_apy((rate_e, rate_e_1): (PoolTokenExchangeRate, PoolTokenExchangeRate)) -> f64 {
    (rate_e.rate() / rate_e_1.rate()).powf(365.0) - 1.0
}
