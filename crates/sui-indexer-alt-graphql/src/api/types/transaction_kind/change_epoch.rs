// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use sui_types::transaction::ChangeEpoch as NativeChangeEpoch;

use crate::{
    api::{
        scalars::{date_time::DateTime, uint53::UInt53},
        types::{epoch::Epoch, protocol_configs::ProtocolConfigs},
    },
    error::RpcError,
    scope::Scope,
};

#[derive(Clone)]
pub(crate) struct ChangeEpochTransaction {
    pub(crate) scope: Scope,
    pub(crate) native: NativeChangeEpoch,
}

// TODO(DVX-1158): Support systemPackages.
/// A system transaction that updates epoch information on-chain (increments the current epoch). Executed by the system once per epoch, without using gas. Epoch change transactions cannot be submitted by users, because validators will refuse to sign them.
///
/// This transaction kind is deprecated in favour of `EndOfEpochTransaction`.
#[Object]
impl ChangeEpochTransaction {
    /// The next (to become) epoch.
    async fn epoch(&self) -> Option<Epoch> {
        Some(Epoch::with_id(self.scope.clone(), self.native.epoch))
    }

    /// The epoch's corresponding protocol configuration.
    async fn protocol_configs(&self) -> Option<ProtocolConfigs> {
        Some(ProtocolConfigs::with_protocol_version(
            self.native.protocol_version.as_u64(),
        ))
    }

    /// The total amount of gas charged for storage during the epoch.
    async fn storage_charge(&self) -> Option<UInt53> {
        Some(self.native.storage_charge.into())
    }

    /// The total amount of gas charged for computation during the epoch.
    async fn computation_charge(&self) -> Option<UInt53> {
        Some(self.native.computation_charge.into())
    }

    /// The amount of storage rebate refunded to the transaction senders.
    async fn storage_rebate(&self) -> Option<UInt53> {
        Some(self.native.storage_rebate.into())
    }

    /// The non-refundable storage fee.
    async fn non_refundable_storage_fee(&self) -> Option<UInt53> {
        Some(self.native.non_refundable_storage_fee.into())
    }

    /// Unix timestamp when epoch started.
    async fn epoch_start_timestamp(&self) -> Result<Option<DateTime>, RpcError> {
        Ok(Some(DateTime::from_ms(
            self.native.epoch_start_timestamp_ms as i64,
        )?))
    }
}
