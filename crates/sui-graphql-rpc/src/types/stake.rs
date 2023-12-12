// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::db_data_provider::PgManager;
use crate::error::Error;

use super::{big_int::BigInt, epoch::Epoch, move_object::MoveObject};
use async_graphql::*;
use sui_json_rpc_types::{Stake as RpcStakedSui, StakeStatus as RpcStakeStatus};
use sui_types::governance::StakedSui as NativeStakedSui;

#[derive(Copy, Clone, Enum, PartialEq, Eq)]
pub(crate) enum StakeStatus {
    /// The stake object is active in a staking pool and it is generating rewards
    Active,
    /// The stake awaits to join a staking pool in the next epoch
    Pending,
    /// The stake is no longer active in any staking pool
    Unstaked,
}

pub(crate) enum StakedSuiDowncastError {
    NotAStakedSui,
    Bcs(bcs::Error),
}

#[derive(Clone)]
pub(crate) struct StakedSui {
    /// Representation of this StakedSui as a generic Move Object.
    pub super_: MoveObject,

    /// Deserialized representation of the Move Object's contents as a
    /// `0x3::staking_pool::StakedSui`.
    pub native: NativeStakedSui,
}

#[Object]
impl StakedSui {
    /// A stake can be pending, active, or unstaked
    async fn status(&self, ctx: &Context<'_>) -> Result<StakeStatus, Error> {
        Ok(match self.rpc_stake(ctx).await?.status {
            RpcStakeStatus::Pending => StakeStatus::Pending,
            RpcStakeStatus::Active { .. } => StakeStatus::Active,
            RpcStakeStatus::Unstaked => StakeStatus::Unstaked,
        })
    }

    /// The epoch at which this stake became active
    async fn active_epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>, Error> {
        Ok(Some(
            ctx.data_unchecked::<PgManager>()
                .fetch_epoch_strict(self.native.activation_epoch())
                .await?,
        ))
    }

    /// The epoch at which this object was requested to join a stake pool
    async fn request_epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>, Error> {
        Ok(Some(
            ctx.data_unchecked::<PgManager>()
                .fetch_epoch_strict(self.native.request_epoch())
                .await?,
        ))
    }

    /// The SUI that was initially staked.
    async fn principal(&self) -> Option<BigInt> {
        Some(BigInt::from(self.native.principal()))
    }

    /// The estimated reward for this stake object, calculated as:
    ///
    ///  principal * (initial_stake_rate / current_stake_rate - 1.0)
    ///
    /// Or 0, if this value is negative, where:
    ///
    /// - `initial_stake_rate` is the stake rate at the epoch this stake was activated at.
    /// - `current_stake_rate` is the stake rate in the current epoch.
    ///
    /// This value is only available if the stake is active.
    async fn estimated_reward(&self, ctx: &Context<'_>) -> Result<Option<BigInt>, Error> {
        let RpcStakeStatus::Active { estimated_reward } = self.rpc_stake(ctx).await?.status else {
            return Ok(None);
        };

        Ok(Some(BigInt::from(estimated_reward)))
    }

    /// The corresponding `0x3::staking_pool::StakedSui` Move object.
    async fn as_move_object(&self) -> &MoveObject {
        &self.super_
    }
}

impl StakedSui {
    /// The JSON-RPC representation of a StakedSui so that we can "cheat" to implement fields that
    /// are not yet implemented directly for GraphQL.
    ///
    /// TODO: Make this obsolete
    async fn rpc_stake(&self, ctx: &Context<'_>) -> Result<RpcStakedSui, Error> {
        ctx.data_unchecked::<PgManager>()
            .fetch_rpc_staked_sui(self.native.clone())
            .await
    }
}

impl TryFrom<&MoveObject> for StakedSui {
    type Error = StakedSuiDowncastError;

    fn try_from(move_object: &MoveObject) -> Result<Self, Self::Error> {
        if !move_object.native.is_staked_sui() {
            return Err(StakedSuiDowncastError::NotAStakedSui);
        }

        Ok(Self {
            super_: move_object.clone(),
            native: bcs::from_bytes(move_object.native.contents())
                .map_err(StakedSuiDowncastError::Bcs)?,
        })
    }
}
