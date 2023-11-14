// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_indexer::models_v2::events::StoredEvent;

use crate::error::Error;

use crate::context_data::db_data_provider::PgManager;

use super::{
<<<<<<< HEAD
    address::Address, base64::Base64, date_time::DateTime, move_module::MoveModule,
    move_type::MoveType, sui_address::SuiAddress,
=======
    address::Address, date_time::DateTime, move_module::MoveModuleId, move_value::MoveValue,
    sui_address::SuiAddress,
>>>>>>> a4603f803f (refactor rust-mirrored Event representation)
};

#[derive(SimpleObject)]
#[graphql(complex)]
pub(crate) struct Event {
<<<<<<< HEAD
    /// Package ID of the Move module that the event was emitted in.
    #[graphql(skip)]
    pub sending_package: SuiAddress,
    /// Name of the module (in `sending_package`) that the event was emitted in.
    #[graphql(skip)]
    pub sending_module: String,
    /// Package, module, and type of the event
    pub event_type: Option<MoveType>,
    pub senders: Option<Vec<Address>>,
=======
    #[graphql(skip)]
    pub stored: StoredEvent,
    #[graphql(flatten)]
    pub contents: MoveValue,
}

#[ComplexObject]
impl Event {
    /// Package id and module name of the Move module that the event was emitted in
    async fn sending_module_id(&self) -> Result<Option<MoveModuleId>> {
        let package_id = SuiAddress::from_bytes(&self.stored.package)
            .map_err(|e| Error::Internal(e.to_string()))
            .extend()?;
        Ok(Some(MoveModuleId {
            package: package_id,
            name: self.stored.module.clone(),
        }))
    }

    /// Addresses of the senders of the event
    async fn senders(&self) -> Result<Option<Vec<Address>>> {
        let result: Result<Option<Vec<Address>>, _> = self
            .stored
            .senders
            .iter()
            .map(|sender| {
                sender
                    .as_ref()
                    .map(|sender| {
                        SuiAddress::from_bytes(sender)
                            .map(|sui_address| Address {
                                address: sui_address,
                            })
                            .map_err(|e| Error::Internal(e.to_string()))
                    })
                    .transpose()
            })
            .collect();

        result.extend()
    }

>>>>>>> a4603f803f (refactor rust-mirrored Event representation)
    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    async fn timestamp(&self) -> Option<DateTime> {
        DateTime::from_ms(self.stored.timestamp_ms)
    }
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

    // Cascading
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

#[ComplexObject]
impl Event {
    /// The Move module that the event was emitted in.
    async fn sending_module(&self, ctx: &Context<'_>) -> Result<Option<MoveModule>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_move_module(self.sending_package, &self.sending_module)
            .await
            .extend()
    }
}
