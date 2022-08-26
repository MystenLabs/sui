// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    GatewayTxSeqNumber, GetObjectDataResponse, GetRawObjectDataResponse, MoveFunctionArgType,
    RPCTransactionRequestParams, SuiEventEnvelope, SuiEventFilter, SuiExecuteTransactionResponse,
    SuiMoveNormalizedFunction, SuiMoveNormalizedModule, SuiMoveNormalizedStruct, SuiObjectInfo,
    SuiTransactionResponse, SuiTypeTag, TransactionBytes,
};
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::crypto::SignatureScheme;
use sui_types::messages::ExecuteTransactionRequestType;
use sui_types::object::Owner;
use sui_types::sui_serde::Base64;

/// Maximum number of events returned in an event query.
/// This is equivalent to EVENT_STORE_QUERY_MAX_LIMIT in `sui-storage` crate.
/// To avoid unnecessary dependency on that crate, we have a reference here
/// for document purposes.
pub const EVENT_QUERY_MAX_LIMIT: usize = 100;

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
        sig_scheme: SignatureScheme,
        /// transaction signature, as base-64 encoded string
        signature: Base64,
        /// signer's public key, as base-64 encoded string
        pub_key: Base64,
    ) -> RpcResult<SuiTransactionResponse>;
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
    ) -> RpcResult<SuiTransactionResponse>;

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
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// the ID of the object to be transferred
        object_id: ObjectID,
        /// gas object to be used in this transaction, the gateway will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: u64,
        /// the recipient's Sui address
        recipient: SuiAddress,
    ) -> RpcResult<TransactionBytes>;

    /// Create an unsigned transaction to send SUI coin object to a Sui address. The SUI object is also used as the gas object.
    #[method(name = "transferSui")]
    async fn transfer_sui(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// the Sui coin object to be used in this transaction
        sui_object_id: ObjectID,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: u64,
        /// the recipient's Sui address
        recipient: SuiAddress,
        /// the amount to be split out and transferred
        amount: Option<u64>,
    ) -> RpcResult<TransactionBytes>;

    /// Create an unsigned transaction to execute a Move call on the network, by calling the specified function in the module of a given package.
    #[method(name = "moveCall")]
    async fn move_call(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// the Move package ID, e.g. `0x2`
        package_object_id: ObjectID,
        /// the Move module name, e.g. `devnet_nft`
        module: String,
        /// the move function name, e.g. `mint`
        function: String,
        /// the type arguments of the Move function
        type_arguments: Vec<SuiTypeTag>,
        /// the arguments to be passed into the Move function, in [SuiJson](https://docs.sui.io/build/sui-json) format
        arguments: Vec<SuiJsonValue>,
        /// gas object to be used in this transaction, the gateway will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Create an unsigned transaction to publish Move module.
    #[method(name = "publish")]
    async fn publish(
        &self,
        /// the transaction signer's Sui address
        sender: SuiAddress,
        /// the compiled bytes of a move module, the
        compiled_modules: Vec<Base64>,
        /// gas object to be used in this transaction, the gateway will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Create an unsigned transaction to split a coin object into multiple coins.
    #[method(name = "splitCoin")]
    async fn split_coin(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// the coin object to be spilt
        coin_object_id: ObjectID,
        /// the amounts to split out from the coin
        split_amounts: Vec<u64>,
        /// gas object to be used in this transaction, the gateway will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Create an unsigned transaction to split a coin object into multiple equal-size coins.
    #[method(name = "splitCoinEqual")]
    async fn split_coin_equal(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// the coin object to be spilt
        coin_object_id: ObjectID,
        /// the number of coins to split into
        split_count: u64,
        /// gas object to be used in this transaction, the gateway will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Create an unsigned transaction to merge multiple coins into one coin.
    #[method(name = "mergeCoins")]
    async fn merge_coin(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// the coin object to merge into, this coin will remain after the transaction
        primary_coin: ObjectID,
        /// the coin object to be merged, this coin will be destroyed, the balance will be added to `primary_coin`
        coin_to_merge: ObjectID,
        /// gas object to be used in this transaction, the gateway will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Create an unsigned batched transaction.
    #[method(name = "batchTransaction")]
    async fn batch_transaction(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// list of transaction request parameters
        single_transaction_params: Vec<RPCTransactionRequestParams>,
        /// gas object to be used in this transaction, the gateway will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
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
    /// Return events emitted by the given transaction.
    #[method(name = "getEventsByTransaction")]
    async fn get_events_by_transaction(
        &self,
        /// digest of the transaction, as base-64 encoded string
        digest: TransactionDigest,
        /// maximum size of the result, capped to EVENT_QUERY_MAX_LIMIT
        count: usize,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    /// Return events emitted in a specified Move module
    #[method(name = "getEventsByModule")]
    async fn get_events_by_transaction_module(
        &self,
        /// the Move package ID
        package: ObjectID,
        /// the module name
        module: String,
        /// maximum size of the result, capped to EVENT_QUERY_MAX_LIMIT
        count: usize,
        /// left endpoint of time interval, inclusive
        start_time: u64,
        /// right endpoint of time interval, exclusive
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    /// Return events with the given move event struct name
    #[method(name = "getEventsByMoveEventStructName")]
    async fn get_events_by_move_event_struct_name(
        &self,
        /// the event struct name type, e.g. `0x2::devnet_nft::MintNFTEvent` or `0x2::SUI::test_foo<address, vector<u8>>` with type params
        move_event_struct_name: String,
        /// maximum size of the result, capped to EVENT_QUERY_MAX_LIMIT
        count: usize,
        /// left endpoint of time interval, inclusive
        start_time: u64,
        /// right endpoint of time interval, exclusive
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    /// Return events associated with the given sender.
    #[method(name = "getEventsBySender")]
    async fn get_events_by_sender(
        &self,
        /// the sender's Sui address
        sender: SuiAddress,
        /// maximum size of the result, capped to EVENT_QUERY_MAX_LIMIT
        count: usize,
        /// left endpoint of time interval, inclusive
        start_time: u64,
        /// right endpoint of time interval, exclusive
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    /// Return events associated with the given recipient
    #[method(name = "getEventsByRecipient")]
    async fn get_events_by_recipient(
        &self,
        /// the recipient
        recipient: Owner,
        /// maximum size of the result, capped to EVENT_QUERY_MAX_LIMIT
        count: usize,
        /// left endpoint of time interval, inclusive
        start_time: u64,
        /// right endpoint of time interval, exclusive
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    /// Return events associated with the given object
    #[method(name = "getEventsByObject")]
    async fn get_events_by_object(
        &self,
        /// the object ID
        object: ObjectID,
        /// maximum size of the result, capped to EVENT_QUERY_MAX_LIMIT
        count: usize,
        /// left endpoint of time interval, inclusive
        start_time: u64,
        /// right endpoint of time interval, exclusive
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    /// Return events emitted in [start_time, end_time) interval
    #[method(name = "getEventsByTimeRange")]
    async fn get_events_by_timerange(
        &self,
        /// maximum size of the result, capped to EVENT_QUERY_MAX_LIMIT
        count: usize,
        /// left endpoint of time interval, inclusive
        start_time: u64,
        /// right endpoint of time interval, exclusive
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;
}

#[open_rpc(namespace = "sui", tag = "Quorum Driver APIs to execute transactions.")]
#[rpc(server, client, namespace = "sui")]
pub trait QuorumDriverApi {
    /// Execute the transaction and wait for results if desired
    #[method(name = "executeTransaction")]
    async fn execute_transaction(
        &self,
        /// transaction data bytes, as base-64 encoded string
        tx_bytes: Base64,
        /// Flag of the signature scheme that is used.
        sig_scheme: SignatureScheme,
        /// transaction signature, as base-64 encoded string
        signature: Base64,
        /// signer's public key, as base-64 encoded string
        pub_key: Base64,
        /// The request type
        request_type: ExecuteTransactionRequestType,
    ) -> RpcResult<SuiExecuteTransactionResponse>;
}
