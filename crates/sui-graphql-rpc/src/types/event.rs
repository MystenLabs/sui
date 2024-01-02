// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_indexer::models_v2::events::StoredEvent;
use sui_types::{event::Event as NativeEvent, parse_sui_struct_tag, TypeTag};

use crate::error::Error;

use crate::context_data::db_data_provider::PgManager;

use super::{
    address::Address, base64::Base64, date_time::DateTime, move_module::MoveModule,
    move_value::MoveValue, sui_address::SuiAddress,
};

pub(crate) struct Event {
    pub stored: Option<StoredEvent>,
    pub native: NativeEvent,
}

#[derive(InputObject, Clone)]
pub(crate) struct EventFilter {
    pub sender: Option<SuiAddress>,
    pub transaction_digest: Option<String>,
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

impl Event {
    fn package_impl(&self) -> Result<SuiAddress, Error> {
        if let Some(stored) = &self.stored {
            SuiAddress::from_bytes(&stored.package).map_err(|e| Error::Internal(e.to_string()))
        } else {
            Ok(SuiAddress::from(self.native.package_id))
        }
    }

    fn module_impl(&self) -> &str {
        if let Some(stored) = &self.stored {
            &stored.module
        } else {
            self.native.transaction_module.as_ident_str().as_str()
        }
    }

    fn timestamp_ms_impl(&self) -> Option<i64> {
        if let Some(stored) = &self.stored {
            Some(stored.timestamp_ms)
        } else {
            None
        }
    }

    fn type_tag_impl(&self) -> Result<TypeTag, Error> {
        let struct_tag = match &self.stored {
            Some(stored) => parse_sui_struct_tag(&stored.event_type)
                .map_err(|e| Error::Internal(e.to_string()))?,
            None => self.native.type_.clone(),
        };

        Ok(TypeTag::from(struct_tag))
    }
}

#[Object]
impl Event {
    /// The Move module containing some function that when called by
    /// a programmable transaction block (PTB) emitted this event.
    /// For example, if a PTB invokes A::m1::foo, which internally
    /// calls A::m2::emit_event to emit an event,
    /// the sending module would be A::m1.
    async fn sending_module(&self, ctx: &Context<'_>) -> Result<Option<MoveModule>> {
        let sending_package = self.package_impl().extend()?;
        let module = self.module_impl();

        ctx.data_unchecked::<PgManager>()
            .fetch_move_module(sending_package, module)
            .await
            .extend()
    }

    /// Addresses of the senders of the event
    async fn senders(&self) -> Result<Option<Vec<Address>>> {
        if let Some(stored) = &self.stored {
            let mut addrs = Vec::with_capacity(stored.senders.len());
            for sender in &stored.senders {
                let Some(sender) = &sender else { continue };
                let address = SuiAddress::from_bytes(sender)
                    .map_err(|e| Error::Internal(format!("Failed to deserialize address: {e}")))
                    .extend()?;
                addrs.push(Address { address });
            }
            Ok(Some(addrs))
        } else {
            Ok(Some(vec![Address {
                address: SuiAddress::from(self.native.sender),
            }]))
        }
    }

    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    async fn timestamp(&self) -> Result<Option<DateTime>, Error> {
        let Some(timestamp_ms) = self.timestamp_ms_impl() else {
            return Ok(None);
        };
        Ok(Some(DateTime::from_ms(timestamp_ms)?))
    }

    #[graphql(flatten)]
    async fn move_value(&self) -> Result<MoveValue> {
        let type_tag = self.type_tag_impl().extend()?;

        Ok(MoveValue::new(
            type_tag,
            Base64::from(self.native.contents.clone()),
        ))
    }
}

impl TryFrom<StoredEvent> for Event {
    type Error = Error;

    fn try_from(stored_event: StoredEvent) -> Result<Self, Error> {
        let native_event: NativeEvent = bcs::from_bytes(&stored_event.bcs).map_err(|_| {
            Error::Internal(format!(
                "Failed to deserialize event with {} at transaction {}",
                stored_event.event_sequence_number, stored_event.tx_sequence_number
            ))
        })?;

        Ok(Self {
            stored: Some(stored_event),
            native: native_event,
        })
    }
}
