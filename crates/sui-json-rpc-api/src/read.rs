// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;

use sui_json_rpc_types::{
    Checkpoint, CheckpointId, CheckpointPage, SuiEvent, SuiGetPastObjectRequest,
    SuiObjectDataOptions, SuiObjectResponse, SuiPastObjectResponse, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_json_rpc_types::{ProtocolConfigResponse, ZkLoginIntentScope, ZkLoginVerifyResult};
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress, TransactionDigest};
use sui_types::sui_serde::BigInt;

#[open_rpc(namespace = "sui", tag = "Read API")]
#[rpc(server, client, namespace = "sui")]
pub trait ReadApi {
    /// Return the transaction response object.
    #[method(name = "getTransactionBlock")]
    async fn get_transaction_block(
        &self,
        /// the digest of the queried transaction
        digest: TransactionDigest,
        /// options for specifying the content to be returned
        options: Option<SuiTransactionBlockResponseOptions>,
    ) -> RpcResult<SuiTransactionBlockResponse>;

    /// Returns an ordered list of transaction responses
    /// The method will throw an error if the input contains any duplicate or
    /// the input size exceeds QUERY_MAX_RESULT_LIMIT
    #[method(name = "multiGetTransactionBlocks")]
    async fn multi_get_transaction_blocks(
        &self,
        /// A list of transaction digests.
        digests: Vec<TransactionDigest>,
        /// config options to control which fields to fetch
        options: Option<SuiTransactionBlockResponseOptions>,
    ) -> RpcResult<Vec<SuiTransactionBlockResponse>>;

    /// Return the object information for a specified object
    #[method(name = "getObject")]
    async fn get_object(
        &self,
        /// the ID of the queried object
        object_id: ObjectID,
        /// options for specifying the content to be returned
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse>;

    /// Return the object data for a list of objects
    #[method(name = "multiGetObjects")]
    async fn multi_get_objects(
        &self,
        /// the IDs of the queried objects
        object_ids: Vec<ObjectID>,
        /// options for specifying the content to be returned
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiObjectResponse>>;

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
    /// Returns the latest object information with a version less than or equal to the given version
    #[method(name = "tryGetObjectBeforeVersion", deprecated = "true")]
    async fn try_get_object_before_version(
        &self,
        /// the ID of the queried object
        object_id: ObjectID,
        /// the version of the queried object
        version: SequenceNumber,
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

    /// Return a checkpoint
    #[method(name = "getCheckpoint")]
    async fn get_checkpoint(
        &self,
        /// Checkpoint identifier, can use either checkpoint digest, or checkpoint sequence number as input.
        id: CheckpointId,
    ) -> RpcResult<Checkpoint>;

    /// Return paginated list of checkpoints
    #[method(name = "getCheckpoints")]
    async fn get_checkpoints(
        &self,
        /// An optional paging cursor. If provided, the query will start from the next item after the specified cursor. Default to start from the first item if not specified.
        cursor: Option<BigInt<u64>>,
        /// Maximum item returned per page, default to [QUERY_MAX_RESULT_LIMIT_CHECKPOINTS] if not specified.
        limit: Option<usize>,
        /// query result ordering, default to false (ascending order), oldest record first.
        descending_order: bool,
    ) -> RpcResult<CheckpointPage>;

    /// Return transaction events.
    #[method(name = "getEvents")]
    async fn get_events(
        &self,
        /// the event query criteria.
        transaction_digest: TransactionDigest,
    ) -> RpcResult<Vec<SuiEvent>>;

    /// Return the total number of transaction blocks known to the server.
    #[method(name = "getTotalTransactionBlocks")]
    async fn get_total_transaction_blocks(&self) -> RpcResult<BigInt<u64>>;

    /// Return the sequence number of the latest checkpoint that has been executed
    #[method(name = "getLatestCheckpointSequenceNumber")]
    async fn get_latest_checkpoint_sequence_number(&self) -> RpcResult<BigInt<u64>>;

    /// Return the protocol config table for the given version number.
    /// If the version number is not specified, If none is specified, the node uses the version of the latest epoch it has processed.
    #[method(name = "getProtocolConfig")]
    async fn get_protocol_config(
        &self,
        /// An optional protocol version specifier. If omitted, the latest protocol config table for the node will be returned.
        version: Option<BigInt<u64>>,
    ) -> RpcResult<ProtocolConfigResponse>;

    /// Return the first four bytes of the chain's genesis checkpoint digest.
    #[method(name = "getChainIdentifier")]
    async fn get_chain_identifier(&self) -> RpcResult<String>;

    /// Verify a zklogin signature for the given bytes, intent scope and author.
    #[method(name = "verifyZkLoginSignature")]
    async fn verify_zklogin_signature(
        &self,
        /// The Base64 string of bcs bytes for raw transaction data or personal message indicated by intent_scope.
        bytes: String,
        /// The Base64 string of the zklogin signature to verify.
        signature: String,
        /// The intent scope, either transaction data or personal message. Used to parse bytes.
        intent_scope: ZkLoginIntentScope,
        /// The author of the signature.
        author: SuiAddress,
    ) -> RpcResult<ZkLoginVerifyResult>;
}
