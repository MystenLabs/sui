// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use sui_types::transaction::ChangeEpoch as NativeChangeEpoch;

#[derive(Clone)]
pub(crate) struct ChangeEpochTransaction {
    pub(crate) native: NativeChangeEpoch,
}

// TODO(DVX-1372): Support systemPackages when MovePackage is available.
/// A system transaction that updates epoch information on-chain (increments the current epoch). Executed by the system once per epoch, without using gas. Epoch change transactions cannot be submitted by users, because validators will refuse to sign them.
///
/// This transaction kind is deprecated in favour of `EndOfEpochTransaction`.
#[Object]
impl ChangeEpochTransaction {
    /// The next (to become) epoch ID.
    async fn epoch(&self) -> Option<u64> {
        Some(self.native.epoch)
    }

    /// The protocol version in effect in the new epoch.
    async fn protocol_version(&self) -> Option<u64> {
        Some(self.native.protocol_version.as_u64())
    }

    /// The total amount of gas charged for storage during the epoch.
    async fn storage_charge(&self) -> Option<u64> {
        Some(self.native.storage_charge)
    }

    /// The total amount of gas charged for computation during the epoch.
    async fn computation_charge(&self) -> Option<u64> {
        Some(self.native.computation_charge)
    }

    /// The amount of storage rebate refunded to the transaction senders.
    async fn storage_rebate(&self) -> Option<u64> {
        Some(self.native.storage_rebate)
    }

    /// The non-refundable storage fee.
    async fn non_refundable_storage_fee(&self) -> Option<u64> {
        Some(self.native.non_refundable_storage_fee)
    }

    /// Unix timestamp when epoch started (in milliseconds).
    async fn epoch_start_timestamp_ms(&self) -> Option<u64> {
        Some(self.native.epoch_start_timestamp_ms)
    }
}
