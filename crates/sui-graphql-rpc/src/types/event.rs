// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use super::{
    address::Address, base64::Base64, date_time::DateTime, move_module::MoveModuleId,
    move_type::MoveType, sui_address::SuiAddress,
};

#[derive(SimpleObject)]
pub(crate) struct Event {
    pub id: ID,
    pub sending_module_id: Option<MoveModuleId>, // Module that the event was emitted by
    pub event_type: Option<MoveType>,
    pub senders: Option<Vec<Address>>,
    pub timestamp: Option<DateTime>,
    pub json: Option<String>,
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
