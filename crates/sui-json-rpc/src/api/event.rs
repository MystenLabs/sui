// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;

use sui_json_rpc_types::{EventFilter, EventPage, SuiEvent};
use sui_open_rpc_macros::open_rpc;
use sui_types::digests::TransactionDigest;
use sui_types::event::EventID;

#[open_rpc(namespace = "sui", tag = "Event Read API")]
#[rpc(server, client, namespace = "sui")]
pub trait EventReadApi {
    /// Return transaction events.
    #[method(name = "getEvents")]
    async fn get_events(
        &self,
        /// the event query criteria.
        transaction_digest: TransactionDigest,
    ) -> RpcResult<Vec<SuiEvent>>;

    /// Return list of events for a specified query criteria.
    #[method(name = "queryEvents")]
    async fn query_events(
        &self,
        /// the event query criteria.
        query: EventFilter,
        /// optional paging cursor
        cursor: Option<EventID>,
        /// maximum number of items per page, default to [QUERY_MAX_RESULT_LIMIT] if not specified.
        limit: Option<usize>,
        /// query result ordering, default to false (ascending order), oldest record first.
        descending_order: Option<bool>,
    ) -> RpcResult<EventPage>;

    /// Subscribe to a stream of Sui event
    #[subscription(name = "subscribeEvent", item = SuiEvent)]
    fn subscribe_event(
        &self,
        /// the filter criteria of the event stream, see the [Sui docs](https://docs.sui.io/build/pubsub#event-filters) for detailed examples.
        filter: EventFilter,
    );
}
