// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_indexer::models_v2::events::StoredEvent;
use sui_types::{parse_sui_struct_tag, TypeTag};

use crate::error::Error;

use crate::context_data::db_data_provider::PgManager;

use super::{
    address::Address, base64::Base64, date_time::DateTime, move_module::MoveModule,
    move_value::MoveValue, sui_address::SuiAddress,
};

pub(crate) struct Event {
    pub stored: StoredEvent,
}

#[derive(InputObject, Clone)]
pub(crate) struct EventFilter {
    pub sender: Option<SuiAddress>,
    pub transaction_digest: Option<String>,
    // Enhancement (post-MVP)
    // after_checkpoint
    // before_checkpoint

    // Cascading
    pub emitting_package: Option<SuiAddress>,
    pub emitting_module: Option<String>,

    pub event_package: Option<SuiAddress>,
    pub event_module: Option<String>,
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
    /// The Move module that the event was emitted in.
    async fn sending_module(&self, ctx: &Context<'_>) -> Result<Option<MoveModule>> {
        let sending_package = SuiAddress::from_bytes(&self.stored.package)
            .map_err(|e| Error::Internal(e.to_string()))
            .extend()?;
        ctx.data_unchecked::<PgManager>()
            .fetch_move_module(sending_package, &self.stored.module)
            .await
            .extend()
    }

    /// Addresses of the senders of the event
    async fn senders(&self) -> Option<Vec<Address>> {
        let result: Option<Vec<Address>> = self
            .stored
            .senders
            .iter()
            .filter_map(|sender| {
                sender.as_ref().map(|sender| {
                    SuiAddress::from_bytes(sender)
                        .map(|sui_address| Address {
                            address: sui_address,
                        })
                        .map_err(|e| eprintln!("Unexpected None value in senders array: {}", e))
                        .ok()
                })
            })
            .collect();

        result
    }

    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    async fn timestamp(&self) -> Option<DateTime> {
        DateTime::from_ms(self.stored.timestamp_ms)
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
