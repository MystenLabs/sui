// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_indexer::models_v2::{events::StoredEvent, transactions::StoredTransaction};
use sui_types::{event::Event as NativeEvent, parse_sui_struct_tag, TypeTag};

use crate::error::Error;

use crate::context_data::db_data_provider::PgManager;

use super::{
    address::Address, base64::Base64, date_time::DateTime, move_module::MoveModule,
    move_value::MoveValue, sui_address::SuiAddress,
};

pub(crate) struct EventFromTransaction {
    pub native: NativeEvent,
    pub timestamp_ms: i64,
}

pub(crate) enum Event {
    Stored(StoredEvent),
    Native(EventFromTransaction),
}

#[derive(InputObject, Clone, Default)]
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
    pub fn try_from(stored_transaction: &StoredTransaction, idx: usize) -> Result<Self, Error> {
        let event = stored_transaction
            .events
            .get(idx as usize)
            .ok_or_else(|| {
                Error::Internal(format!(
                    "Failed to get event with {} at transaction {}",
                    idx, stored_transaction.tx_sequence_number
                ))
            })?
            .as_ref()
            .ok_or_else(|| {
                Error::Internal(format!(
                    "Failed to get event with {} at transaction {}",
                    idx, stored_transaction.tx_sequence_number
                ))
            })?;

        let native_event: NativeEvent = bcs::from_bytes(&event).map_err(|_| {
            Error::Internal(format!(
                "Failed to deserialize event with {} at transaction {}",
                idx, stored_transaction.tx_sequence_number
            ))
        })?;

        Ok(Self::Native(EventFromTransaction {
            native: native_event,
            timestamp_ms: stored_transaction.timestamp_ms,
        }))
    }

    fn package_impl(&self) -> Result<SuiAddress, Error> {
        match self {
            Self::Stored(stored) => SuiAddress::from_bytes(&stored.package)
                .map_err(|e| Error::Internal(format!("Failed to deserialize address: {e}"))),
            Self::Native(e) => Ok(SuiAddress::from(e.native.package_id)),
        }
    }

    fn module_impl(&self) -> &str {
        match self {
            Self::Stored(stored) => &stored.module,
            Self::Native(e) => e.native.transaction_module.as_ident_str().as_str(),
        }
    }

    fn timestamp_ms_impl(&self) -> i64 {
        match self {
            Self::Stored(stored) => stored.timestamp_ms,
            Self::Native(e) => e.timestamp_ms,
        }
    }

    fn type_tag_impl(&self) -> Result<TypeTag, Error> {
        let struct_tag = match self {
            Self::Stored(stored) => parse_sui_struct_tag(&stored.event_type)
                .map_err(|e| Error::Internal(e.to_string()))?,
            Self::Native(e) => e.native.type_.clone(),
        };

        Ok(TypeTag::from(struct_tag))
    }

    fn bcs_impl(&self) -> &Vec<u8> {
        match self {
            Self::Stored(stored) => &stored.bcs,
            Self::Native(e) => &e.native.contents,
        }
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
        match self {
            Self::Stored(stored) => {
                let mut addrs = Vec::with_capacity(stored.senders.len());
                for sender in &stored.senders {
                    let Some(sender) = &sender else { continue };
                    let address = SuiAddress::from_bytes(sender)
                        .map_err(|e| Error::Internal(format!("Failed to deserialize address: {e}")))
                        .extend()?;
                    addrs.push(Address { address });
                }
                Ok(Some(addrs))
            }
            Self::Native(e) => Ok(Some(vec![Address {
                address: SuiAddress::from(e.native.sender),
            }])),
        }
    }

    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    async fn timestamp(&self) -> Result<Option<DateTime>, Error> {
        Ok(Some(DateTime::from_ms(self.timestamp_ms_impl())?))
    }

    #[graphql(flatten)]
    async fn move_value(&self) -> Result<MoveValue> {
        let type_tag = self.type_tag_impl().extend()?;

        Ok(MoveValue::new(type_tag, Base64::from(self.bcs_impl())))
    }
}

impl From<StoredEvent> for Event {
    fn from(stored_event: StoredEvent) -> Self {
        Self::Stored(stored_event)
    }
}
