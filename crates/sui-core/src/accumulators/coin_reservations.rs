// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use moka::sync::Cache as MokaCache;
use move_core_types::language_storage::TypeTag;
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    coin_reservation::{
        CoinReservationResolver, CoinReservationResolverTrait, ParsedObjectRefWithdrawal,
    },
    error::{UserInputError, UserInputResult},
    storage::ChildObjectResolver,
    transaction::FundsWithdrawalArg,
};

/// A caching wrapper around `CoinReservationResolver` that caches the lookup
/// of (owner, type_tag) for each accumulator object ID.
pub struct CachingCoinReservationResolver {
    inner: CoinReservationResolver,
    cache: MokaCache<ObjectID, Result<(SuiAddress, TypeTag), UserInputError>>,
}

impl CachingCoinReservationResolver {
    pub fn new(child_object_resolver: Arc<dyn ChildObjectResolver + Send + Sync>) -> Self {
        Self {
            inner: CoinReservationResolver::new(child_object_resolver),
            cache: MokaCache::builder().max_capacity(1000).build(),
        }
    }

    fn get_owner_and_type_cached(
        &self,
        object_id: ObjectID,
        accumulator_version: Option<SequenceNumber>,
    ) -> UserInputResult<(SuiAddress, TypeTag)> {
        // Owner and type_tag never change, so the cache is always coherent.
        // On cache miss, use MVCC to read at the specified version.
        self.cache.get_with(object_id, || {
            self.inner
                .get_owner_and_type_for_object(object_id, accumulator_version)
        })
    }

    pub fn resolve_funds_withdrawal(
        &self,
        sender: SuiAddress,
        coin_reservation: ParsedObjectRefWithdrawal,
        accumulator_version: Option<SequenceNumber>,
    ) -> UserInputResult<FundsWithdrawalArg> {
        let (owner, type_tag) = self
            .get_owner_and_type_cached(coin_reservation.unmasked_object_id, accumulator_version)?;

        if sender != owner {
            return Err(UserInputError::InvalidWithdrawReservation {
                error: format!(
                    "coin reservation object id {} is owned by {}, not sender {}",
                    coin_reservation.unmasked_object_id, owner, sender
                ),
            });
        }

        Ok(FundsWithdrawalArg::balance_from_sender(
            coin_reservation.reservation_amount(),
            type_tag,
        ))
    }
}

impl CoinReservationResolverTrait for CachingCoinReservationResolver {
    fn resolve_funds_withdrawal(
        &self,
        sender: SuiAddress,
        coin_reservation: ParsedObjectRefWithdrawal,
        accumulator_version: Option<SequenceNumber>,
    ) -> UserInputResult<FundsWithdrawalArg> {
        CachingCoinReservationResolver::resolve_funds_withdrawal(
            self,
            sender,
            coin_reservation,
            accumulator_version,
        )
    }
}

impl CoinReservationResolverTrait for &'_ CachingCoinReservationResolver {
    fn resolve_funds_withdrawal(
        &self,
        sender: SuiAddress,
        coin_reservation: ParsedObjectRefWithdrawal,
        accumulator_version: Option<SequenceNumber>,
    ) -> UserInputResult<FundsWithdrawalArg> {
        CachingCoinReservationResolver::resolve_funds_withdrawal(
            self,
            sender,
            coin_reservation,
            accumulator_version,
        )
    }
}
