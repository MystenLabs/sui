// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Object, SimpleObject};
use sui_types::sui_system_state::sui_system_state_inner_v1::ValidatorV1;

use crate::api::scalars::{
    base64::Base64, big_int::BigInt, sui_address::SuiAddress, uint53::UInt53,
};

#[derive(Clone, Debug)]
pub(crate) struct Validator {
    native: ValidatorV1,
}

/// The credentials related fields associated with a validator.
#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct ValidatorCredentials {
    pub protocol_pub_key: Option<Base64>,
    pub network_pub_key: Option<Base64>,
    pub worker_pub_key: Option<Base64>,
    pub proof_of_possession: Option<Base64>,
    pub net_address: Option<String>,
    pub p2p_address: Option<String>,
    pub primary_address: Option<String>,
    pub worker_address: Option<String>,
}

// todo (ewall) implement IAddressable
#[Object]
impl Validator {
    /// The validator's address.
    async fn address(&self) -> SuiAddress {
        self.native.metadata.sui_address.into()
    }

    /// Validator's set of credentials such as public keys, network addresses and others.
    async fn credentials(&self) -> Option<ValidatorCredentials> {
        let v = &self.native.metadata;
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
        let v = &self.native.metadata;
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
        Some(self.native.metadata.name.clone())
    }

    /// Validator's description.
    async fn description(&self) -> Option<String> {
        Some(self.native.metadata.description.clone())
    }

    /// Validator's url containing their custom image.
    async fn image_url(&self) -> Option<String> {
        Some(self.native.metadata.image_url.clone())
    }

    /// Validator's homepage URL.
    async fn project_url(&self) -> Option<String> {
        Some(self.native.metadata.project_url.clone())
    }

    // todo (ewall)
    // /// The validator's current valid `Cap` object. Validators can delegate
    // /// the operation ability to another address. The address holding this `Cap` object
    // /// can then update the reference gas price and tallying rule on behalf of the validator.
    // async fn operation_cap(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<MoveObject>> {
    //     MoveObject::query(
    //         ctx,
    //         self.operation_cap_id(),
    //         Object::latest_at(self.checkpoint_viewed_at),
    //     )
    //         .await
    //         .extend()
    // }

    /// The ID of this validator's `0x3::staking_pool::StakingPool`.
    async fn staking_pool_id(&self) -> SuiAddress {
        self.native.staking_pool.id.into()
    }

    // todo (ewall)
    // /// A wrapped object containing the validator's exchange rates. This is a table from epoch
    // /// number to `PoolTokenExchangeRate` value. The exchange rate is used to determine the amount
    // /// of SUI tokens that each past SUI staker can withdraw in the future.
    // async fn exchange_rates_table(&self) -> async_graphql::Result<Option<Owner>> {
    //     Ok(Some(Owner {
    //         address: self.validator_summary.exchange_rates_id.into(),
    //         checkpoint_viewed_at: self.checkpoint_viewed_at,
    //         root_version: None,
    //     }))
    // }

    /// Number of exchange rates in the table.
    async fn exchange_rates_size(&self) -> Option<UInt53> {
        Some(self.native.staking_pool.exchange_rates.size.into())
    }

    /// The epoch at which this pool became active.
    async fn staking_pool_activation_epoch(&self) -> Option<UInt53> {
        self.native.staking_pool.activation_epoch.map(UInt53::from)
    }

    /// The total number of SUI tokens in this pool.
    async fn staking_pool_sui_balance(&self) -> Option<BigInt> {
        Some(BigInt::from(self.native.staking_pool.sui_balance))
    }

    /// The epoch stake rewards will be added here at the end of each epoch.
    async fn rewards_pool(&self) -> Option<BigInt> {
        Some(BigInt::from(self.native.staking_pool.rewards_pool.value()))
    }

    /// Total number of pool tokens issued by the pool.
    async fn pool_token_balance(&self) -> Option<BigInt> {
        Some(BigInt::from(self.native.staking_pool.pool_token_balance))
    }

    /// Pending stake amount for this epoch.
    async fn pending_stake(&self) -> Option<BigInt> {
        Some(BigInt::from(self.native.staking_pool.pending_stake))
    }

    /// Pending stake withdrawn during the current epoch, emptied at epoch boundaries.
    async fn pending_total_sui_withdraw(&self) -> Option<BigInt> {
        Some(BigInt::from(
            self.native.staking_pool.pending_total_sui_withdraw,
        ))
    }

    /// Pending pool token withdrawn during the current epoch, emptied at epoch boundaries.
    async fn pending_pool_token_withdraw(&self) -> Option<BigInt> {
        Some(BigInt::from(
            self.native.staking_pool.pending_pool_token_withdraw,
        ))
    }

    /// The voting power of this validator in basis points (e.g., 100 = 1% voting power).
    async fn voting_power(&self) -> Option<u64> {
        Some(self.native.voting_power)
    }

    /// The reference gas price for this epoch.
    async fn gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(self.native.gas_price))
    }

    /// The fee charged by the validator for staking services.
    async fn commission_rate(&self) -> Option<u64> {
        Some(self.native.commission_rate)
    }

    /// The total number of SUI tokens in this pool plus the pending stake amount for this epoch.
    async fn next_epoch_stake(&self) -> Option<BigInt> {
        Some(BigInt::from(self.native.next_epoch_stake))
    }

    /// The validator's gas price quote for the next epoch.
    async fn next_epoch_gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(self.native.next_epoch_gas_price))
    }

    /// The proposed next epoch fee for the validator's staking services.
    async fn next_epoch_commission_rate(&self) -> Option<u64> {
        Some(self.native.next_epoch_commission_rate)
    }

    // todo (ewall)
    // /// The number of epochs for which this validator has been below the
    // /// low stake threshold.
    // async fn at_risk(&self) -> Option<UInt53> {
    //     self.at_risk.map(UInt53::from)
    // }

    // todo (ewall)
    // /// The addresses of other validators this validator has reported.
    // async fn report_records(
    //     &self,
    //     ctx: &Context<'_>,
    //     first: Option<u64>,
    //     before: Option<CAddr>,
    //     last: Option<u64>,
    //     after: Option<CAddr>,
    // ) -> async_graphql::Result<Connection<String, Address>> {
    //     let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
    //
    //     let mut connection = Connection::new(false, false);
    //     let Some(addresses) = &self.report_records else {
    //         return Ok(connection);
    //     };
    //
    //     let Some((prev, next, _, cs)) =
    //         page.paginate_consistent_indices(addresses.len(), self.checkpoint_viewed_at)?
    //     else {
    //         return Ok(connection);
    //     };
    //
    //     connection.has_previous_page = prev;
    //     connection.has_next_page = next;
    //
    //     for c in cs {
    //         connection.edges.push(Edge::new(
    //             c.encode_cursor(),
    //             Address {
    //                 address: addresses[c.ix].address,
    //                 checkpoint_viewed_at: c.c,
    //             },
    //         ));
    //     }
    //
    //     Ok(connection)
    // }
}

impl From<ValidatorV1> for Validator {
    fn from(native: ValidatorV1) -> Self {
        Self { native }
    }
}
