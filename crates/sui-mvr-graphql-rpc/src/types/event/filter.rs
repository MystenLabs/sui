// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::{
    digest::Digest,
    sui_address::SuiAddress,
    type_filter::{ModuleFilter, TypeFilter},
};
use async_graphql::*;

#[derive(InputObject, Clone, Default)]
pub(crate) struct EventFilter {
    /// Filter down to events from transactions sent by this address.
    pub sender: Option<SuiAddress>,

    /// Filter down to the events from this transaction (given by its transaction digest).
    pub transaction_digest: Option<Digest>,

    // Enhancement (post-MVP)
    // after_checkpoint
    // at_checkpoint
    // before_checkpoint
    /// Events emitted by a particular module. An event is emitted by a
    /// particular module if some function in the module is called by a
    /// PTB and emits an event.
    ///
    /// Modules can be filtered by their package, or package::module.
    /// We currently do not support filtering by emitting module and event type
    /// at the same time so if both are provided in one filter, the query will error.
    pub emitting_module: Option<ModuleFilter>,

    /// This field is used to specify the type of event emitted.
    ///
    /// Events can be filtered by their type's package, package::module,
    /// or their fully qualified type name.
    ///
    /// Generic types can be queried by either the generic type name, e.g.
    /// `0x2::coin::Coin`, or by the full type name, such as
    /// `0x2::coin::Coin<0x2::sui::SUI>`.
    pub event_type: Option<TypeFilter>,
    // Enhancement (post-MVP)
    // pub start_time
    // pub end_time
}
