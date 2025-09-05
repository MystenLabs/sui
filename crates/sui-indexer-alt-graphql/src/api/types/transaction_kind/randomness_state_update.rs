// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use sui_types::transaction::RandomnessStateUpdate as NativeRandomnessStateUpdate;

use crate::api::scalars::base64::Base64;

#[derive(Clone)]
pub(crate) struct RandomnessStateUpdateTransaction {
    pub(crate) native: NativeRandomnessStateUpdate,
}

/// System transaction to update the source of on-chain randomness.
#[Object]
impl RandomnessStateUpdateTransaction {
    /// Epoch of the randomness state update transaction.
    async fn epoch(&self) -> Option<u64> {
        Some(self.native.epoch)
    }

    /// Randomness round of the update.
    async fn randomness_round(&self) -> Option<u64> {
        Some(self.native.randomness_round.0)
    }

    /// Updated random bytes, Base64 encoded.
    async fn random_bytes(&self) -> Option<Base64> {
        Some(Base64::from(self.native.random_bytes.clone()))
    }

    /// The initial version of the randomness object that it was shared at.
    async fn randomness_obj_initial_shared_version(&self) -> Option<u64> {
        Some(self.native.randomness_obj_initial_shared_version.value())
    }
}
