// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::{base64::Base64, epoch::Epoch, uint53::UInt53};
use async_graphql::*;
use sui_types::transaction::RandomnessStateUpdate as NativeRandomnessStateUpdate;

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct RandomnessStateUpdateTransaction {
    pub native: NativeRandomnessStateUpdate,
    /// The checkpoint sequence number this was viewed at.
    pub checkpoint_viewed_at: u64,
}

/// System transaction to update the source of on-chain randomness.
#[Object]
impl RandomnessStateUpdateTransaction {
    /// Epoch of the randomness state update transaction.
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        Epoch::query(ctx, Some(self.native.epoch), self.checkpoint_viewed_at)
            .await
            .extend()
    }

    /// Randomness round of the update.
    async fn randomness_round(&self) -> UInt53 {
        self.native.randomness_round.0.into()
    }

    /// Updated random bytes, encoded as Base64.
    async fn random_bytes(&self) -> Base64 {
        Base64::from(&self.native.random_bytes)
    }

    /// The initial version the randomness object was shared at.
    async fn randomness_obj_initial_shared_version(&self) -> UInt53 {
        self.native
            .randomness_obj_initial_shared_version
            .value()
            .into()
    }
}
