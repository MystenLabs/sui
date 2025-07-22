// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use async_graphql::Object;
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress, digests::TransactionDigest,
    event::Event as NativeEvent,
};

use crate::{
    api::scalars::{base64::Base64, date_time::DateTime, uint53::UInt53},
    error::RpcError,
    scope::Scope,
};

use super::{address::Address, transaction::Transaction};

#[derive(Clone)]
pub(crate) struct Event {
    pub(crate) scope: Scope,
    pub(crate) native: NativeEvent,
    /// Digest of the transaction that emitted this event
    pub(crate) transaction_digest: TransactionDigest,
    /// Position of this event within the transaction's events list (0-indexed)
    pub(crate) sequence_number: u64,
    /// Timestamp when the transaction containing this event was finalized (checkpoint time)
    pub(crate) timestamp_ms: u64,
}

// TODO(DVX-1200): Support sendingModule - MoveModule
// TODO(DVX-1203): contents - MoveValue
#[Object]
impl Event {
    /// The Base64 encoded BCS serialized bytes of the entire event structure.
    async fn event_bcs(&self) -> Result<Option<Base64>, RpcError> {
        let bcs_bytes = bcs::to_bytes(&self.native).context("Failed to serialize event")?;
        Ok(Some(Base64(bcs_bytes)))
    }

    /// Address of the sender of the event.
    async fn sender(&self) -> Option<Address> {
        if self.native.sender == NativeSuiAddress::ZERO {
            return None;
        }

        Some(Address::with_address(
            self.scope.clone(),
            self.native.sender,
        ))
    }

    /// The position of the event among the events from the same transaction — useful for distinguishing between events with the same content that appeared from the same transaction.
    async fn sequence_number(&self) -> UInt53 {
        UInt53::from(self.sequence_number)
    }

    /// Timestamp corresponding to the checkpoint this event's transaction was finalized in.
    /// All events from the same transaction share the same timestamp.
    async fn timestamp(&self) -> Result<Option<DateTime>, RpcError> {
        Ok(Some(DateTime::from_ms(self.timestamp_ms as i64)?))
    }

    /// The transaction that emitted this event. This information is only available for events from indexed transactions, and not from transactions that have just been executed or dry-run.
    async fn transaction(&self) -> Option<Transaction> {
        Some(Transaction::with_id(
            self.scope.clone(),
            self.transaction_digest,
        ))
    }
}

impl Event {
    /// Create an Event from native Sui types with additional context.
    ///
    /// # Arguments
    /// * `scope` - GraphQL query scope for data consistency
    /// * `native` - The native Sui event structure
    /// * `transaction_digest` - Digest of the transaction that emitted this event
    /// * `sequence_number` - Position of this event in the transaction's events list
    /// * `timestamp_ms` - Checkpoint timestamp when the transaction was finalized
    pub(crate) fn from_native(
        scope: Scope,
        native: NativeEvent,
        transaction_digest: TransactionDigest,
        sequence_number: u64,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            scope,
            native,
            transaction_digest,
            sequence_number,
            timestamp_ms,
        }
    }
}
