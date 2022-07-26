// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    GatewayTxSeqNumber, GetObjectDataResponse, GetRawObjectDataResponse,
    RPCTransactionRequestParams, SuiEventEnvelope, SuiEventFilter, SuiObjectInfo, SuiTypeTag,
    TransactionBytes, TransactionEffectsResponse, TransactionResponse,
};
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::sui_serde::Base64;

#[open_rpc(namespace = "sui", tag = "Gateway Transaction Execution API")]
#[rpc(server, client, namespace = "sui")]
pub trait RpcGatewayApi {
    /// Execute the transaction using the transaction data, signature and public key.
    #[method(name = "executeTransaction")]
    async fn execute_transaction(
        &self,
        /// transaction data bytes, as base-64 encoded string
        tx_bytes: Base64,
        /// Flag of the signature scheme that is used.
        flag: Base64,
        /// transaction signature, as base-64 encoded string
        signature: Base64,
        /// signer's public key, as base-64 encoded string
        pub_key: Base64,
    ) -> RpcResult<TransactionResponse>;
}

#[open_rpc(namespace = "sui", tag = "Wallet Sync API")]
#[rpc(server, client, namespace = "sui")]
pub trait WalletSyncApi {
    /// Synchronize client state with validators.
    #[method(name = "syncAccountState")]
    async fn sync_account_state(
        &self,
        /// the Sui address to be synchronized
        address: SuiAddress,
    ) -> RpcResult<()>;
}

#[open_rpc(namespace = "sui", tag = "Read API")]
#[rpc(server, client, namespace = "sui")]
pub trait RpcReadApi {
    /// Return the list of objects owned by an address.
    #[method(name = "getObjectsOwnedByAddress")]
    async fn get_objects_owned_by_address(
        &self,
        /// the owner's Sui address
        address: SuiAddress,
    ) -> RpcResult<Vec<SuiObjectInfo>>;

    /// Return the list of objects owned by an object.
    #[method(name = "getObjectsOwnedByObject")]
    async fn get_objects_owned_by_object(
        &self,
        /// the ID of the owner object
        object_id: ObjectID,
    ) -> RpcResult<Vec<SuiObjectInfo>>;

    /// Return the total number of transactions known to the server.
    #[method(name = "getTotalTransactionNumber")]
    async fn get_total_transaction_number(&self) -> RpcResult<u64>;

    /// Return list of transaction digests within the queried range.
    #[method(name = "getTransactionsInRange")]
    async fn get_transactions_in_range(
        &self,
        /// the matching transactions' sequence number will be greater than or equals to the starting sequence number
        start: GatewayTxSeqNumber,
        /// the matching transactions' sequence number will be less than the ending sequence number
        end: GatewayTxSeqNumber,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    /// Return list of recent transaction digest.
    #[method(name = "getRecentTransactions")]
    async fn get_recent_transactions(
        &self,
        /// maximum size of the result
        count: u64,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    /// Return the transaction response object.
    #[method(name = "getTransaction")]
    async fn get_transaction(
        &self,
        /// the digest of the queried transaction
        digest: TransactionDigest,
    ) -> RpcResult<TransactionEffectsResponse>;

    /// Return the object information for a specified object
    #[method(name = "getObject")]
    async fn get_object(
        &self,
        /// the ID of the queried object
        object_id: ObjectID,
    ) -> RpcResult<GetObjectDataResponse>;
}

#[open_rpc(namespace = "sui", tag = "Full Node API")]
#[rpc(server, client, namespace = "sui")]
pub trait RpcFullNodeReadApi {
    /// Return list of transactions for a specified input object.
    #[method(name = "getTransactionsByInputObject")]
    async fn get_transactions_by_input_object(
        &self,
        /// the ID of the input object
        object: ObjectID,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    /// Return list of transactions for a specified mutated object.
    #[method(name = "getTransactionsByMutatedObject")]
    async fn get_transactions_by_mutated_object(
        &self,
        /// the ID of the mutated object
        object: ObjectID,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    /// Return list of transactions for a specified move function.
    #[method(name = "getTransactionsByMoveFunction")]
    async fn get_transactions_by_move_function(
        &self,
        /// the Move package ID, e.g. `0x2`
        package: ObjectID,
        /// the Move module name, e.g. `devnet_nft`
        module: Option<String>,
        /// the move function name, e.g. `mint`
        function: Option<String>,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    /// Return list of transactions for a specified sender's Sui address.
    #[method(name = "getTransactionsFromAddress")]
    async fn get_transactions_from_addr(
        &self,
        /// the sender's Sui address
        addr: SuiAddress,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    /// Return list of transactions for a specified recipient's Sui address.
    #[method(name = "getTransactionsToAddress")]
    async fn get_transactions_to_addr(
        &self,
        /// the recipient's Sui address
        addr: SuiAddress,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;
}

#[open_rpc(namespace = "sui", tag = "Transaction Builder API")]
#[rpc(server, client, namespace = "sui")]
pub trait RpcTransactionBuilder {
    /// Create an unsigned transaction to transfer an object from one address to another. The object's type
    /// must allow public transfers
    #[method(name = "transferObject")]
    async fn transfer_object(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> RpcResult<TransactionBytes>;

    /// Create an unsigned transaction to send SUI coin object to a Sui address. The SUI object is also used as the gas object.
    #[method(name = "transferSui")]
    async fn transfer_sui(
        &self,
        signer: SuiAddress,
        sui_object_id: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
        amount: Option<u64>,
    ) -> RpcResult<TransactionBytes>;

    /// Create an unsigned transaction to execute a Move call on the network, by calling the specified function in the module of a given package.
    #[method(name = "moveCall")]
    async fn move_call(
        &self,
        signer: SuiAddress,
        package_object_id: ObjectID,
        module: String,
        function: String,
        type_arguments: Vec<SuiTypeTag>,
        arguments: Vec<SuiJsonValue>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Create an unsigned transaction to publish Move module.
    #[method(name = "publish")]
    async fn publish(
        &self,
        sender: SuiAddress,
        compiled_modules: Vec<Base64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Create an unsigned transaction to split a coin object into multiple coins.
    #[method(name = "splitCoin")]
    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Create an unsigned transaction to merge multiple coins into one coin.
    #[method(name = "mergeCoins")]
    async fn merge_coin(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Create an unsigned batched transaction.
    #[method(name = "batchTransaction")]
    async fn batch_transaction(
        &self,
        signer: SuiAddress,
        single_transaction_params: Vec<RPCTransactionRequestParams>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;
}

#[open_rpc(namespace = "sui", tag = "BCS API")]
#[rpc(server, client, namespace = "sui")]
pub trait RpcBcsApi {
    /// Return the raw BCS serialized move object bytes for a specified object.
    #[method(name = "getRawObject")]
    async fn get_raw_object(
        &self,
        /// the id of the object
        object_id: ObjectID,
    ) -> RpcResult<GetRawObjectDataResponse>;
}

#[open_rpc(namespace = "sui", tag = "Event Subscription")]
#[rpc(server, client, namespace = "sui")]
pub trait EventStreamingApi {
    /// Subscribe to a stream of Sui event
    #[subscription(name = "subscribeEvent", item = SuiEventEnvelope)]
    fn subscribe_event(
        &self,
        /// the filter criteria of the event stream, see the [Sui docs](https://docs.sui.io/build/pubsub#event-filters) for detailed examples.
        filter: SuiEventFilter,
    );
}

#[open_rpc(namespace = "sui", tag = "Event Read API")]
#[rpc(server, client, namespace = "sui")]
pub trait EventReadApi {
    /// Return list of events emitted by a specified transaction.
    #[method(name = "getEventsByTransaction")]
    async fn get_events_by_transaction(
        &self,
        /// digest of the transaction, as base-64 encoded string
        digest: TransactionDigest,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    /// Return list of events emitted by a specified Move module
    #[method(name = "getEventsByModule")]
    async fn get_events_by_module(
        &self,
        /// the Move package ID
        package: ObjectID,
        /// the module name
        module: String,
        /// maximum size of the result
        count: u64,
        /// the matching events' timestamp will be after the specified start time
        start_time: u64,
        /// the matching events' timestamp will be before the specified end time
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    /// Return list of events matching the specified event type
    #[method(name = "getEventsByEventType")]
    async fn get_events_by_event_type(
        &self,
        /// the event type, e.g. '0x2::devnet_nft::MintNFTEvent'
        event_type: String,
        /// maximum size of the result
        count: u64,
        /// the matching events' timestamp will be after the specified start time
        start_time: u64,
        /// the matching events' timestamp will be before the specified end time
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    /// Return list of events involving a specified sender.
    #[method(name = "getEventsBySender")]
    async fn get_events_by_sender(
        &self,
        /// the sender's Sui address
        sender: SuiAddress,
        /// maximum size of the result
        count: u64,
        /// the matching events' timestamp will be after the specified start time
        start_time: u64,
        /// the matching events' timestamp will be before the specified end time
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    /// Return list of events involving a specified object
    #[method(name = "getEventsByObject")]
    async fn get_events_by_object(
        &self,
        /// the object ID
        object: ObjectID,
        /// maximum size of the result
        count: u64,
        /// the matching events' timestamp will be after the specified start time
        start_time: u64,
        /// the matching events' timestamp will be before the specified end time
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    /// Return list of events involving a specified owner.
    #[method(name = "getEventsByOwner")]
    async fn get_events_by_owner(
        &self,
        /// the owner's Sui address
        owner: SuiAddress,
        /// maximum size of the result
        count: u64,
        /// the matching events' timestamp will be after the specified start time
        start_time: u64,
        /// the matching events' timestamp will be before the specified end time
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;
}
