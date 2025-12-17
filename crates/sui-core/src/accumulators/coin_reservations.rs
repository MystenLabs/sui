// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use moka::sync::Cache as MokaCache;
use sui_types::{
    accumulator_root::{AccumulatorKey, AccumulatorValue},
    base_types::{ObjectID, SuiAddress},
    coin_reservation::{CoinReservationResolverTrait, ParsedObjectRefWithdrawal},
    error::{UserInputError, UserInputResult},
    storage::ChildObjectResolver,
    transaction::FundsWithdrawalArg,
    type_input::TypeInput,
};

macro_rules! invalid_res_error {
    ($($args:tt)*) => {
        UserInputError::InvalidWithdrawReservation {
            error: format!($($args)*),
        }
    };
}

pub struct CoinReservationResolver {
    child_object_resolver: Arc<dyn ChildObjectResolver + Send + Sync>,
    object_id_to_type_cache: MokaCache<ObjectID, (SuiAddress, TypeInput)>,
}

impl CoinReservationResolver {
    pub fn new(child_object_resolver: Arc<dyn ChildObjectResolver + Send + Sync>) -> Self {
        Self {
            child_object_resolver,
            object_id_to_type_cache: MokaCache::builder().max_capacity(1000).build(),
        }
    }

    fn get_type_input_for_object(
        &self,
        sender: SuiAddress,
        object_id: ObjectID,
    ) -> UserInputResult<TypeInput> {
        let (owner, type_input) = self
            .object_id_to_type_cache
            .try_get_with(object_id, || -> UserInputResult<(SuiAddress, TypeInput)> {
                // Load accumulator field object
                let object = AccumulatorValue::load_object_by_id(
                    self.child_object_resolver.as_ref(),
                    None,
                    object_id,
                )
                .map_err(|e| invalid_res_error!("could not load coin reservation object id {}", e))?
                .ok_or_else(|| {
                    invalid_res_error!("coin reservation object id {} not found", object_id)
                })?;

                let move_object = object.data.try_as_move().unwrap();

                // Get the balance type
                let type_input: TypeInput = move_object
                    .type_()
                    .balance_accumulator_field_type_maybe()
                    .ok_or_else(|| {
                        invalid_res_error!(
                            "coin reservation object id {} is not a balance accumulator field",
                            object_id
                        )
                    })?
                    .into();

                // get the owner
                let (key, _): (AccumulatorKey, AccumulatorValue) =
                    move_object.try_into().map_err(|e| {
                        invalid_res_error!("could not load coin reservation object id {}", e)
                    })?;
                Ok((key.owner, type_input))
            })
            .map_err(|e| (*e).clone())?;

        if sender != owner {
            return Err(invalid_res_error!(
                "coin reservation object id {} is owned by {}, not sender {}",
                object_id,
                owner,
                sender
            ));
        }

        Ok(type_input)
    }

    pub fn resolve_funds_withdrawal(
        &self,
        sender: SuiAddress,
        coin_reservation: ParsedObjectRefWithdrawal,
    ) -> UserInputResult<FundsWithdrawalArg> {
        let type_input =
            self.get_type_input_for_object(sender, coin_reservation.unmasked_object_id)?;

        Ok(FundsWithdrawalArg::balance_from_sender(
            coin_reservation.reservation_amount(),
            type_input,
        ))
    }
}

impl CoinReservationResolverTrait for CoinReservationResolver {
    fn resolve_funds_withdrawal(
        &self,
        sender: SuiAddress,
        coin_reservation: ParsedObjectRefWithdrawal,
    ) -> UserInputResult<FundsWithdrawalArg> {
        self.resolve_funds_withdrawal(sender, coin_reservation)
    }
}

impl CoinReservationResolverTrait for &'_ CoinReservationResolver {
    fn resolve_funds_withdrawal(
        &self,
        sender: SuiAddress,
        coin_reservation: ParsedObjectRefWithdrawal,
    ) -> UserInputResult<FundsWithdrawalArg> {
        CoinReservationResolver::resolve_funds_withdrawal(self, sender, coin_reservation)
    }
}
