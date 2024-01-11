// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_indexer::models_v2::events::StoredEvent;
use sui_indexer::models_v2::transactions::StoredTransaction;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
use sui_types::event::Event as NativeEvent;
use sui_types::{parse_sui_struct_tag, TypeTag};

use crate::error::Error;

use super::digest::Digest;
use super::{
    address::Address, base64::Base64, date_time::DateTime, move_module::MoveModule,
    move_value::MoveValue, sui_address::SuiAddress,
};

pub(crate) struct Event {
    pub stored: StoredEvent,
}

#[derive(InputObject, Clone, Default)]
pub(crate) struct EventFilter {
    pub sender: Option<SuiAddress>,
    pub transaction_digest: Option<Digest>,
    // Enhancement (post-MVP)
    // after_checkpoint
    // before_checkpoint
    /// Events emitted by a particular module. An event is emitted by a
    /// particular module if some function in the module is called by a
    /// PTB and emits an event.
    ///
    /// Modules can be filtered by their package, or package::module.
    pub emitting_module: Option<String>,

    /// This field is used to specify the type of event emitted.
    ///
    /// Events can be filtered by their type's package, package::module,
    /// or their fully qualified type name.
    ///
    /// Generic types can be queried by either the generic type name, e.g.
    /// `0x2::coin::Coin`, or by the full type name, such as
    /// `0x2::coin::Coin<0x2::sui::SUI>`.
    pub event_type: Option<String>,
    // Enhancement (post-MVP)
    // pub start_time
    // pub end_time

    // Enhancement (post-MVP)
    // pub any
    // pub all
    // pub not
}

#[Object]
impl Event {
    /// The Move module containing some function that when called by
    /// a programmable transaction block (PTB) emitted this event.
    /// For example, if a PTB invokes A::m1::foo, which internally
    /// calls A::m2::emit_event to emit an event,
    /// the sending module would be A::m1.
    async fn sending_module(&self, ctx: &Context<'_>) -> Result<Option<MoveModule>> {
        let sending_package = SuiAddress::from_bytes(&self.stored.package)
            .map_err(|e| Error::Internal(e.to_string()))
            .extend()?;
        MoveModule::query(ctx.data_unchecked(), sending_package, &self.stored.module)
            .await
            .extend()
    }

    /// Address of the sender of the event
    async fn sender(&self) -> Result<Option<Address>> {
        let Some(Some(sender)) = self.stored.senders.first() else {
            return Ok(None);
        };

        let address = SuiAddress::from_bytes(sender)
            .map_err(|e| Error::Internal(format!("Failed to deserialize address: {e}")))
            .extend()?;

        if address.as_slice() == NativeSuiAddress::ZERO.as_ref() {
            return Ok(None);
        }

        Ok(Some(Address { address }))
    }

    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    async fn timestamp(&self) -> Result<Option<DateTime>, Error> {
        Ok(DateTime::from_ms(self.stored.timestamp_ms).ok())
    }

    #[graphql(flatten)]
    async fn move_value(&self) -> Result<MoveValue> {
        let type_ = TypeTag::from(
            parse_sui_struct_tag(&self.stored.event_type)
                .map_err(|e| Error::Internal(e.to_string()))
                .extend()?,
        );
        Ok(MoveValue::new(type_, Base64::from(self.stored.bcs.clone())))
    }
}

impl Event {
    pub(crate) fn try_from_stored_transaction(
        stored_tx: &StoredTransaction,
        idx: usize,
    ) -> Result<Self, Error> {
        let Some(Some(serialized_event)) = stored_tx.events.get(idx) else {
            return Err(Error::Internal(format!(
                "Could not find event with event_sequence_number {} at transaction {}",
                idx, stored_tx.tx_sequence_number
            )));
        };

        let native_event: NativeEvent = bcs::from_bytes(serialized_event).map_err(|_| {
            Error::Internal(format!(
                "Failed to deserialize event with {} at transaction {}",
                idx, stored_tx.tx_sequence_number
            ))
        })?;

        let stored_event = StoredEvent {
            tx_sequence_number: stored_tx.tx_sequence_number,
            event_sequence_number: idx as i64,
            transaction_digest: stored_tx.transaction_digest.clone(),
            checkpoint_sequence_number: stored_tx.checkpoint_sequence_number,
            senders: vec![Some(native_event.sender.to_vec())],
            package: native_event.package_id.to_vec(),
            module: native_event.transaction_module.to_string(),
            event_type: native_event
                .type_
                .to_canonical_string(/* with_prefix */ true),
            bcs: native_event.contents,
            timestamp_ms: stored_tx.timestamp_ms,
        };

        Ok(Self {
            stored: stored_event,
        })
    }
}
