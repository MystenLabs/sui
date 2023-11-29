// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use crate::context_data::db_data_provider::PgManager;

use super::{
    address::Address, base64::Base64, date_time::DateTime, move_module::MoveModule,
    move_type::MoveType, sui_address::SuiAddress,
};

#[derive(SimpleObject)]
#[graphql(complex)]
pub(crate) struct Event {
    /// Package ID of the Move module that the event was emitted in.
    #[graphql(skip)]
    pub sending_package: SuiAddress,
    /// Name of the module (in `sending_package`) that the event was emitted in.
    #[graphql(skip)]
    pub sending_module: String,
    /// Package, module, and type of the event
    pub event_type: Option<MoveType>,
    pub senders: Option<Vec<Address>>,
    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    pub timestamp: Option<DateTime>,
    /// JSON string representation of the event
    pub json: Option<String>,
    /// Base64 encoded bcs bytes of the Move event
    pub bcs: Option<Base64>,
}

#[derive(InputObject)]
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
