// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::{Context, Object, SimpleObject, connection::Connection};
use sui_types::sui_system_state::sui_system_state_inner_v1::ValidatorV1;

use crate::{
    api::{
        scalars::{
            base64::Base64, big_int::BigInt, cursor::JsonCursor, sui_address::SuiAddress,
            type_filter::TypeInput, uint53::UInt53,
        },
        types::{
            address::Address,
            balance,
            balance::Balance,
            move_object::MoveObject,
            object::{CLive, Error},
            object_filter::{ObjectFilter, ObjectFilterValidator as OFValidator},
            validator_set::ValidatorSetContents,
        },
    },
    error::{RpcError, upcast},
    pagination::{Page, PaginationConfig},
};

#[derive(Clone, Debug)]
pub(crate) struct Validator {
    pub(crate) contents: Arc<ValidatorSetContents>,
    pub(crate) idx: usize,
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

type CAddr = JsonCursor<usize>;

#[Object]
impl Validator {
    /// The validator's address.
    pub(crate) async fn address(&self, ctx: &Context<'_>) -> Result<SuiAddress, RpcError> {
        self.super_().address(ctx).await
    }

    /// Fetch the total balance for coins with marker type `coinType` (e.g. `0x2::sui::SUI`), owned by this address.
    ///
    /// If the address does not own any coins of that type, a balance of zero is returned.
    pub(crate) async fn balance(
        &self,
        ctx: &Context<'_>,
        coin_type: TypeInput,
    ) -> Result<Option<Balance>, RpcError<balance::Error>> {
        self.super_().balance(ctx, coin_type).await
    }

    /// Total balance across coins owned by this address, grouped by coin type.
    pub(crate) async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Result<Option<Connection<String, Balance>>, RpcError<balance::Error>> {
        self.super_()
            .balances(ctx, first, after, last, before)
            .await
    }

    /// The domain explicitly configured as the default SuiNS name for this address.
    pub(crate) async fn default_suins_name(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<String>, RpcError> {
        self.super_().default_suins_name(ctx).await
    }

    /// Fetch the total balances keyed by coin types (e.g. `0x2::sui::SUI`) owned by this address.
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    /// If the address does not own any coins of a given type, a balance of zero is returned for that type.
    pub(crate) async fn multi_get_balances(
        &self,
        ctx: &Context<'_>,
        keys: Vec<TypeInput>,
    ) -> Result<Option<Vec<Balance>>, RpcError<balance::Error>> {
        self.super_().multi_get_balances(ctx, keys).await
    }

    /// Objects owned by this object, optionally filtered by type.
    pub(crate) async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CLive>,
        last: Option<u64>,
        before: Option<CLive>,
        #[graphql(validator(custom = "OFValidator::allows_empty()"))] filter: Option<ObjectFilter>,
    ) -> Result<Option<Connection<String, MoveObject>>, RpcError<Error>> {
        self.super_()
            .objects(ctx, first, after, last, before, filter)
            .await
    }

    /// Validator's set of credentials such as public keys, network addresses and others.
    async fn credentials(&self) -> Option<ValidatorCredentials> {
        let v = &self.validator().metadata;
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
        let v = &self.validator().metadata;
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
        Some(self.validator().metadata.name.clone())
    }

    /// Validator's description.
    async fn description(&self) -> Option<String> {
        Some(self.validator().metadata.description.clone())
    }

    /// Validator's url containing their custom image.
    async fn image_url(&self) -> Option<String> {
        Some(self.validator().metadata.image_url.clone())
    }

    /// Validator's homepage URL.
    async fn project_url(&self) -> Option<String> {
        Some(self.validator().metadata.project_url.clone())
    }

    /// The validator's current valid `Cap` object. Validators can delegate the operation ability to another address.
    /// The address holding this `Cap` object can then update the reference gas price and tallying rule on behalf of the validator.
    async fn operation_cap(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<MoveObject>, RpcError<Error>> {
        let address = Address::with_address(
            self.contents.scope.clone(),
            self.validator().operation_cap_id.bytes.into(),
        );
        let Some(object) = address.as_object(ctx).await? else {
            return Ok(None);
        };
        object.as_move_object(ctx).await.map_err(upcast)
    }

    /// The ID of this validator's `0x3::staking_pool::StakingPool`.
    async fn staking_pool_id(&self) -> SuiAddress {
        self.validator().staking_pool.id.into()
    }

    /// A wrapped object containing the validator's exchange rates. This is a table from epoch number to `PoolTokenExchangeRate` value.
    /// The exchange rate is used to determine the amount of SUI tokens that each past SUI staker can withdraw in the future.
    async fn exchange_rates_table(&self) -> Option<Address> {
        let address = Address::with_address(
            self.contents.scope.clone(),
            self.validator().staking_pool.exchange_rates.id.into(),
        );
        Some(address)
    }

    /// Number of exchange rates in the table.
    async fn exchange_rates_size(&self) -> Option<UInt53> {
        Some(self.validator().staking_pool.exchange_rates.size.into())
    }

    /// The epoch at which this pool became active.
    async fn staking_pool_activation_epoch(&self) -> Option<UInt53> {
        self.validator()
            .staking_pool
            .activation_epoch
            .map(UInt53::from)
    }

    /// The total number of SUI tokens in this pool.
    async fn staking_pool_sui_balance(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator().staking_pool.sui_balance))
    }

    /// The epoch stake rewards will be added here at the end of each epoch.
    async fn rewards_pool(&self) -> Option<BigInt> {
        Some(BigInt::from(
            self.validator().staking_pool.rewards_pool.value(),
        ))
    }

    /// Total number of pool tokens issued by the pool.
    async fn pool_token_balance(&self) -> Option<BigInt> {
        Some(BigInt::from(
            self.validator().staking_pool.pool_token_balance,
        ))
    }

    /// Pending stake amount for this epoch.
    async fn pending_stake(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator().staking_pool.pending_stake))
    }

    /// Pending stake withdrawn during the current epoch, emptied at epoch boundaries.
    async fn pending_total_sui_withdraw(&self) -> Option<BigInt> {
        Some(BigInt::from(
            self.validator().staking_pool.pending_total_sui_withdraw,
        ))
    }

    /// Pending pool token withdrawn during the current epoch, emptied at epoch boundaries.
    async fn pending_pool_token_withdraw(&self) -> Option<BigInt> {
        Some(BigInt::from(
            self.validator().staking_pool.pending_pool_token_withdraw,
        ))
    }

    /// The voting power of this validator in basis points (e.g., 100 = 1% voting power).
    async fn voting_power(&self) -> Option<u64> {
        Some(self.validator().voting_power)
    }

    /// The reference gas price for this epoch.
    async fn gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator().gas_price))
    }

    /// The fee charged by the validator for staking services.
    async fn commission_rate(&self) -> Option<u64> {
        Some(self.validator().commission_rate)
    }

    /// The total number of SUI tokens in this pool plus the pending stake amount for this epoch.
    async fn next_epoch_stake(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator().next_epoch_stake))
    }

    /// The validator's gas price quote for the next epoch.
    async fn next_epoch_gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(self.validator().next_epoch_gas_price))
    }

    /// The proposed next epoch fee for the validator's staking services.
    async fn next_epoch_commission_rate(&self) -> Option<u64> {
        Some(self.validator().next_epoch_commission_rate)
    }

    /// The number of epochs for which this validator has been below the low stake threshold.
    async fn at_risk(&self) -> Option<UInt53> {
        let at_risk = self
            .contents
            .native
            .at_risk_validators
            .get(&self.validator().metadata.sui_address)
            .map_or(0, |at_risk| *at_risk);
        Some(at_risk.into())
    }

    /// Other validators this validator has reported.
    async fn report_records(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        before: Option<CAddr>,
        last: Option<u64>,
        after: Option<CAddr>,
    ) -> Result<Option<Connection<String, Validator>>, RpcError> {
        let Some(report_records) = self
            .contents
            .report_records
            .get(&self.validator().metadata.sui_address)
        else {
            return Ok(Some(Connection::new(false, false)));
        };

        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Validator", "reportRecords");
        let page = Page::from_params(limits, first, after, last, before)?;
        page.paginate_indices(report_records.len(), |i| {
            let idx = report_records[i];
            Ok(Validator {
                contents: Arc::clone(&self.contents),
                idx,
            })
        })
        .map(Some)
    }
}

impl Validator {
    fn super_(&self) -> Address {
        Address::with_address(
            self.contents.scope.clone(),
            self.validator().metadata.sui_address,
        )
    }

    fn validator(&self) -> &ValidatorV1 {
        &self.contents.native.active_validators[self.idx]
    }
}
