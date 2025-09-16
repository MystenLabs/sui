// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use moka::sync::Cache as MokaCache;
use sui_types::{
    base_types::{ObjectID, ObjectRef},
    coin::CoinMetadata,
    coin_reservation::{self, CoinReservationResolverTrait},
    error::{UserInputError, UserInputResult},
    transaction::FundsWithdrawalArg,
    type_input::TypeInput,
    TypeTag,
};

use crate::execution_cache::ObjectCacheRead;

macro_rules! invalid_res_error {
    ($($args:tt)*) => {
        UserInputError::InvalidWithdrawReservation {
            error: format!($($args)*),
        }
    };
}

pub struct CoinReservationResolver {
    object_cache_read: Arc<dyn ObjectCacheRead>,
    object_id_to_type_cache: MokaCache<ObjectID, TypeInput>,
}

impl CoinReservationResolver {
    pub fn new(object_cache_read: Arc<dyn ObjectCacheRead>) -> Self {
        Self {
            object_cache_read,
            object_id_to_type_cache: MokaCache::builder().max_capacity(1000).build(),
        }
    }

    fn get_type_input_for_object(&self, object_id: &ObjectID) -> UserInputResult<TypeInput> {
        let type_input = self.object_id_to_type_cache.get(object_id);
        if let Some(type_input) = type_input {
            return Ok(type_input);
        }

        let object = self
            .object_cache_read
            .get_object(object_id)
            .ok_or_else(|| invalid_res_error!("object id {} not found", object_id))?;

        // Ensure that transaction is referencing an object that can never be deleted or wrapped
        match object.owner() {
            sui_types::object::Owner::Shared { .. } | sui_types::object::Owner::Immutable => (),
            _ => {
                return Err(invalid_res_error!(
                    "object id {} must be shared or immutable",
                    object_id
                ));
            }
        }

        let object_type: TypeTag = object
            .type_()
            .ok_or_else(|| invalid_res_error!("object id {} is not a move object", object_id))?
            .clone()
            .into();

        let TypeTag::Struct(object_struct) = object_type else {
            return Err(invalid_res_error!(
                "object id {} is not a coin metadata object",
                object_id
            ));
        };

        let coin_struct = CoinMetadata::is_coin_metadata_with_coin_type(&object_struct)
            .ok_or_else(|| invalid_res_error!("object id {} is not a coin metadata", object_id))?;
        let coin_type = TypeTag::Struct(coin_struct.clone().into());
        let coin_type_input = TypeInput::from(coin_type);
        self.object_id_to_type_cache
            .insert(*object_id, coin_type_input.clone());
        Ok(coin_type_input)
    }

    pub fn resolve_funds_withdrawal(
        &self,
        coin_reservation: ObjectRef,
    ) -> UserInputResult<FundsWithdrawalArg> {
        // Should only be called on valid coin reservation object refs
        let parsed = coin_reservation::parse_object_ref(&coin_reservation)
            .expect("invalid coin reservation object ref");

        // object existence must be checked earlier
        let type_input = self.get_type_input_for_object(&parsed.unmasked_object_id)?;

        Ok(FundsWithdrawalArg::balance_from_sender(
            parsed.reservation_amount,
            type_input,
        ))
    }
}

impl CoinReservationResolverTrait for CoinReservationResolver {
    fn resolve_funds_withdrawal(
        &self,
        coin_reservation: ObjectRef,
    ) -> UserInputResult<FundsWithdrawalArg> {
        self.resolve_funds_withdrawal(coin_reservation)
    }
}

impl CoinReservationResolverTrait for &'_ CoinReservationResolver {
    fn resolve_funds_withdrawal(
        &self,
        coin_reservation: ObjectRef,
    ) -> UserInputResult<FundsWithdrawalArg> {
        CoinReservationResolver::resolve_funds_withdrawal(self, coin_reservation)
    }
}
