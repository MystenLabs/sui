// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;

use sui_json_rpc_types::{
<<<<<<< HEAD
    DynamicFieldPage, EventFilter, EventPage, ObjectsPage, SuiEvent, SuiObjectResponse,
    SuiObjectResponseQuery, SuiTransactionBlockResponseQuery, TransactionBlocksPage,
=======
    CheckpointedObjectID, DynamicFieldPage, EventFilter, EventPage, ObjectsPage, SuiEvent,
    SuiObjectResponse, SuiObjectResponseQuery, SuiTransactionBlockResponseQuery,
    TransactionBlocksPage,
>>>>>>> fork/testnet
};
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::digests::TransactionDigest;
use sui_types::dynamic_field::DynamicFieldName;
use sui_types::event::EventID;

#[open_rpc(namespace = "suix", tag = "Extended API")]
#[rpc(server, client, namespace = "suix")]
pub trait IndexerApi {
    /// Return the list of objects owned by an address.
<<<<<<< HEAD
    /// Note that if the address owns more than `QUERY_MAX_RESULT_LIMIT_OBJECTS` objects,
    /// the pagination is not accurate, because previous page may have been updated when
    /// the next page is fetched.
    /// Please use suix_queryObjects if this is a concern.
    #[method(name = "getOwnedObjects", blocking)]
    fn get_owned_objects(
=======
    #[method(name = "getOwnedObjects")]
    async fn get_owned_objects(
>>>>>>> fork/testnet
        &self,
        /// the owner's Sui address
        address: SuiAddress,
        /// the objects query criteria.
        query: Option<SuiObjectResponseQuery>,
        /// An optional paging cursor. If provided, the query will start from the next item after the specified cursor. Default to start from the first item if not specified.
<<<<<<< HEAD
        cursor: Option<ObjectID>,
=======
        cursor: Option<CheckpointedObjectID>,
>>>>>>> fork/testnet
        /// Max number of items returned per page, default to [QUERY_MAX_RESULT_LIMIT_OBJECTS] if not specified.
        limit: Option<usize>,
    ) -> RpcResult<ObjectsPage>;

    /// Return list of transactions for a specified query criteria.
<<<<<<< HEAD
    #[method(name = "queryTransactionBlocks", blocking)]
    fn query_transaction_blocks(
=======
    #[method(name = "queryTransactionBlocks")]
    async fn query_transaction_blocks(
>>>>>>> fork/testnet
        &self,
        /// the transaction query criteria.
        query: SuiTransactionBlockResponseQuery,
        /// An optional paging cursor. If provided, the query will start from the next item after the specified cursor. Default to start from the first item if not specified.
        cursor: Option<TransactionDigest>,
        /// Maximum item returned per page, default to QUERY_MAX_RESULT_LIMIT if not specified.
        limit: Option<usize>,
        /// query result ordering, default to false (ascending order), oldest record first.
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionBlocksPage>;

    /// Return list of events for a specified query criteria.
<<<<<<< HEAD
    #[method(name = "queryEvents", blocking)]
    fn query_events(
=======
    #[method(name = "queryEvents")]
    async fn query_events(
>>>>>>> fork/testnet
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

    /// Return the list of dynamic field objects owned by an object.
<<<<<<< HEAD
    #[method(name = "getDynamicFields", blocking)]
    fn get_dynamic_fields(
=======
    #[method(name = "getDynamicFields")]
    async fn get_dynamic_fields(
>>>>>>> fork/testnet
        &self,
        /// The ID of the parent object
        parent_object_id: ObjectID,
        /// An optional paging cursor. If provided, the query will start from the next item after the specified cursor. Default to start from the first item if not specified.
        cursor: Option<ObjectID>,
        /// Maximum item returned per page, default to [QUERY_MAX_RESULT_LIMIT] if not specified.
        limit: Option<usize>,
    ) -> RpcResult<DynamicFieldPage>;

    /// Return the dynamic field object information for a specified object
<<<<<<< HEAD
    #[method(name = "getDynamicFieldObject", blocking)]
    fn get_dynamic_field_object(
=======
    #[method(name = "getDynamicFieldObject")]
    async fn get_dynamic_field_object(
>>>>>>> fork/testnet
        &self,
        /// The ID of the queried parent object
        parent_object_id: ObjectID,
        /// The Name of the dynamic field
        name: DynamicFieldName,
    ) -> RpcResult<SuiObjectResponse>;
}
