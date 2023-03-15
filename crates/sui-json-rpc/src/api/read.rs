// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;
use std::collections::BTreeMap;
use sui_json_rpc_types::{
    BigInt, Checkpoint, CheckpointId, DynamicFieldPage, MoveFunctionArgType, ObjectsPage,
    SuiGetPastObjectRequest, SuiMoveNormalizedFunction, SuiMoveNormalizedModule,
    SuiMoveNormalizedStruct, SuiObjectDataOptions, SuiObjectResponse, SuiPastObjectResponse,
    SuiTransactionResponse, SuiTransactionResponseOptions, SuiTransactionResponseQuery,
    TransactionsPage,
};
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::{
    ObjectID, SequenceNumber, SuiAddress, TransactionDigest, TxSequenceNumber,
};
use sui_types::dynamic_field::DynamicFieldName;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

#[open_rpc(namespace = "sui", tag = "Read API")]
#[rpc(server, client, namespace = "sui")]
pub trait ReadApi {
    /// Return the list of objects owned by an address.
    #[method(name = "getOwnedObjects")]
    async fn get_owned_objects(
        &self,
        /// the owner's Sui address
        address: SuiAddress,
        /// options for specifying the content to be returned
        options: Option<SuiObjectDataOptions>,
        /// Optional paging cursor
        cursor: Option<ObjectID>,
        /// Max number of items returned per page, default to [MAX_GET_OWNED_OBJECT_SIZE] if not specified.
        limit: Option<usize>,
        /// If not specified, objects may be created or deleted across pagination requests. This parameter is only supported when the sui-indexer instance is running.
        at_checkpoint: Option<CheckpointId>,
    ) -> RpcResult<ObjectsPage>;

    /// Return the list of dynamic field objects owned by an object.
    #[method(name = "getDynamicFields")]
    async fn get_dynamic_fields(
        &self,
        /// The ID of the parent object
        parent_object_id: ObjectID,
        /// Optional paging cursor
        cursor: Option<ObjectID>,
        /// Maximum item returned per page, default to [QUERY_MAX_RESULT_LIMIT] if not specified.
        limit: Option<usize>,
    ) -> RpcResult<DynamicFieldPage>;

    /// Return the total number of transactions known to the server.
    #[method(name = "getTotalTransactionNumber")]
    async fn get_total_transaction_number(&self) -> RpcResult<BigInt>;

    /// Return list of transaction digests within the queried range.
    /// This method will be removed before April 2023, please use `queryTransactions` instead
    #[method(name = "getTransactionsInRangeDeprecated", deprecated)]
    async fn get_transactions_in_range_deprecated(
        &self,
        /// the matching transactions' sequence number will be greater than or equals to the starting sequence number
        start: TxSequenceNumber,
        /// the matching transactions' sequence number will be less than the ending sequence number
        end: TxSequenceNumber,
    ) -> RpcResult<Vec<TransactionDigest>>;

    /// Return the transaction response object.
    #[method(name = "getTransaction")]
    async fn get_transaction_with_options(
        &self,
        /// the digest of the queried transaction
        digest: TransactionDigest,
        /// options for specifying the content to be returned
        options: Option<SuiTransactionResponseOptions>,
    ) -> RpcResult<SuiTransactionResponse>;

    /// Return the object information for a specified object
    #[method(name = "getObject")]
    async fn get_object_with_options(
        &self,
        /// the ID of the queried object
        object_id: ObjectID,
        /// options for specifying the content to be returned
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse>;

    /// Return the object data for a list of objects
    #[method(name = "multiGetObjects")]
    async fn multi_get_object_with_options(
        &self,
        /// the IDs of the queried objects
        object_ids: Vec<ObjectID>,
        /// options for specifying the content to be returned
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiObjectResponse>>;

    /// Return the dynamic field object information for a specified object
    #[method(name = "getDynamicFieldObject")]
    async fn get_dynamic_field_object(
        &self,
        /// The ID of the queried parent object
        parent_object_id: ObjectID,
        /// The Name of the dynamic field
        name: DynamicFieldName,
    ) -> RpcResult<SuiObjectResponse>;

    /// Return the argument types of a Move function,
    /// based on normalized Type.
    #[method(name = "getMoveFunctionArgTypes")]
    async fn get_move_function_arg_types(
        &self,
        package: ObjectID,
        module: String,
        function: String,
    ) -> RpcResult<Vec<MoveFunctionArgType>>;

    /// Return structured representations of all modules in the given package
    #[method(name = "getNormalizedMoveModulesByPackage")]
    async fn get_normalized_move_modules_by_package(
        &self,
        package: ObjectID,
    ) -> RpcResult<BTreeMap<String, SuiMoveNormalizedModule>>;

    /// Return a structured representation of Move module
    #[method(name = "getNormalizedMoveModule")]
    async fn get_normalized_move_module(
        &self,
        package: ObjectID,
        module_name: String,
    ) -> RpcResult<SuiMoveNormalizedModule>;

    /// Return a structured representation of Move struct
    #[method(name = "getNormalizedMoveStruct")]
    async fn get_normalized_move_struct(
        &self,
        package: ObjectID,
        module_name: String,
        struct_name: String,
    ) -> RpcResult<SuiMoveNormalizedStruct>;

    /// Return a structured representation of Move function
    #[method(name = "getNormalizedMoveFunction")]
    async fn get_normalized_move_function(
        &self,
        package: ObjectID,
        module_name: String,
        function_name: String,
    ) -> RpcResult<SuiMoveNormalizedFunction>;

    /// Return list of transactions for a specified query criteria.
    #[method(name = "queryTransactions")]
    async fn query_transactions(
        &self,
        /// the transaction query criteria.
        query: SuiTransactionResponseQuery,
        /// Optional paging cursor
        cursor: Option<TransactionDigest>,
        /// Maximum item returned per page, default to QUERY_MAX_RESULT_LIMIT if not specified.
        limit: Option<usize>,
        /// query result ordering, default to false (ascending order), oldest record first.
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionsPage>;

    /// Returns an ordered list of transaction responses
    /// The method will throw an error if the input contains any duplicate or
    /// the input size exceeds QUERY_MAX_RESULT_LIMIT
    #[method(name = "multiGetTransactions")]
    async fn multi_get_transactions_with_options(
        &self,
        /// A list of transaction digests.
        digests: Vec<TransactionDigest>,
        /// config options to control which fields to fetch
        options: Option<SuiTransactionResponseOptions>,
    ) -> RpcResult<Vec<SuiTransactionResponse>>;

    /// Note there is no software-level guarantee/SLA that objects with past versions
    /// can be retrieved by this API, even if the object and version exists/existed.
    /// The result may vary across nodes depending on their pruning policies.
    /// Return the object information for a specified version
    #[method(name = "tryGetPastObject")]
    async fn try_get_past_object(
        &self,
        /// the ID of the queried object
        object_id: ObjectID,
        /// the version of the queried object. If None, default to the latest known version
        version: SequenceNumber,
        /// options for specifying the content to be returned
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiPastObjectResponse>;

    /// Note there is no software-level guarantee/SLA that objects with past versions
    /// can be retrieved by this API, even if the object and version exists/existed.
    /// The result may vary across nodes depending on their pruning policies.
    /// Return the object information for a specified version
    #[method(name = "tryMultiGetPastObjects")]
    async fn try_multi_get_past_objects(
        &self,
        /// a vector of object and versions to be queried
        past_objects: Vec<SuiGetPastObjectRequest>,
        /// options for specifying the content to be returned
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiPastObjectResponse>>;

    /// Return the sequence number of the latest checkpoint that has been executed
    #[method(name = "getLatestCheckpointSequenceNumber")]
    async fn get_latest_checkpoint_sequence_number(&self) -> RpcResult<CheckpointSequenceNumber>;

    /// Return a checkpoint
    #[method(name = "getCheckpoint")]
    async fn get_checkpoint(
        &self,
        /// Checkpoint identifier, can use either checkpoint digest, or checkpoint sequence number as input.
        id: CheckpointId,
    ) -> RpcResult<Checkpoint>;
}
