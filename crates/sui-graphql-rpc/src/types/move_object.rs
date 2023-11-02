// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::big_int::BigInt;
use super::move_value::MoveValue;
use super::stake::StakeStatus;
use super::{coin::Coin, object::Object};
use crate::context_data::db_data_provider::PgManager;
use crate::error::Error;
use crate::types::stake::Stake;
use async_graphql::*;
use move_core_types::language_storage::TypeTag;
use sui_types::governance::StakedSui;
use sui_types::object::Object as NativeSuiObject;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct MoveObject {
    pub native_object: NativeSuiObject,
}

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl MoveObject {
    /// Displays the contents of the MoveObject in a json string and through graphql types
    /// Also provides the flat representation of the type signature, and the bcs of the corresponding data
    async fn contents(&self, ctx: &Context<'_>) -> Result<Option<MoveValue>> {
        let resolver = ctx.data_unchecked::<PgManager>();

        if let Some(struct_tag) = self.native_object.data.struct_tag() {
            let type_tag = TypeTag::Struct(Box::new(struct_tag));
            return Ok(Some(MoveValue::new(
                type_tag.to_string(),
                self.native_object
                    .data
                    .try_as_move()
                    .ok_or_else(|| {
                        Error::Internal(format!(
                            "Failed to convert native object to move object: {}",
                            self.native_object.id()
                        ))
                    })?
                    .contents()
                    .into(),
            )));
        }

        Ok(None)
    }

    /// Determines whether a tx can transfer this object
    async fn has_public_transfer(&self) -> Option<bool> {
        self.native_object
            .data
            .try_as_move()
            .map(|x| x.has_public_transfer())
    }

    /// Attempts to convert the Move object into an Object
    /// This provides additional information such as version and digest on the top-level
    async fn as_object(&self) -> Option<Object> {
        Some(Object::from(&self.native_object))
    }

    /// Attempts to convert the Move object into a Coin
    async fn as_coin(&self) -> Option<Coin> {
        let move_object = self.native_object.data.try_as_move()?;

        if !move_object.is_coin() {
            return None;
        }

        Some(Coin {
            move_obj: self.clone(),
            balance: None, // Defer to resolver
        })
    }

    /// Attempts to convert the Move object into a Stake
    async fn as_stake(&self, ctx: &Context<'_>) -> Result<Option<Stake>> {
        let Some(move_object) = self.native_object.data.try_as_move() else {
            return Ok(None);
        };

        if !move_object.is_staked_sui() {
            return Ok(None);
        }

        let stake: StakedSui = bcs::from_bytes(move_object.contents())
            .map_err(|e| Error::Internal(format!("Failed to deserialized Staked Sui: {e}")))?;

        let latest_system_state = ctx
            .data_unchecked::<PgManager>()
            .fetch_latest_sui_system_state()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;

        let current_epoch_id = latest_system_state.epoch_id;

        let status = if current_epoch_id >= stake.activation_epoch() {
            StakeStatus::Active
        } else {
            StakeStatus::Pending
        };

        Ok(Some(Stake {
            active_epoch_id: Some(stake.activation_epoch()),
            estimated_reward: None,
            principal: Some(BigInt::from(stake.principal())),
            request_epoch_id: Some(stake.activation_epoch().saturating_sub(1)),
            status: Some(status),
            staked_sui_id: stake.id(),
        }))
    }
}
