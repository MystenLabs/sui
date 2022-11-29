// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::collections::BTreeMap;

use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;

use fastcrypto::encoding::Base64;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    EventPage, GetObjectDataResponse, GetPastObjectDataResponse, GetRawObjectDataResponse,
    MoveFunctionArgType, RPCTransactionRequestParams, SuiCoinMetadata, SuiEventEnvelope,
    SuiEventFilter, SuiExecuteTransactionResponse, SuiGasCostSummary, SuiMoveNormalizedFunction,
    SuiMoveNormalizedModule, SuiMoveNormalizedStruct, SuiObjectInfo, SuiTransactionEffects,
    SuiTransactionFilter, SuiTransactionResponse, SuiTypeTag, TransactionBytes, TransactionsPage,
};
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress, TransactionDigest};
use sui_types::batch::TxSequenceNumber;
use sui_types::committee::EpochId;
use sui_types::crypto::SignatureScheme;
use sui_types::event::EventID;
use sui_types::messages::CommitteeInfoResponse;
use sui_types::messages::ExecuteTransactionRequestType;
use sui_types::query::{EventQuery, TransactionQuery};

/// Maximum number of events returned in an event query.
/// This is equivalent to EVENT_QUERY_MAX_LIMIT in `sui-storage` crate.
/// To avoid unnecessary dependency on that crate, we have a reference here
/// for document purposes.
pub const QUERY_MAX_RESULT_LIMIT: usize = 1000;

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
        start: TxSequenceNumber,
        /// the matching transactions' sequence number will be less than the ending sequence number
        end: TxSequenceNumber,
    ) -> RpcResult<Vec<TransactionDigest>>;

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
    #[method(name = "dryRunTransaction")]
    async fn dry_run_transaction(&self, tx_bytes: Base64) -> RpcResult<SuiTransactionEffects>;

    /// Return metadata(e.g., symbol, decimals) for a coin
    #[method(name = "getCoinMetadata")]
    async fn get_coin_metadata(
        &self,
        /// fully qualified type names for the coin (e.g., 0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC)
        coin_type: String,
    ) -> RpcResult<SuiCoinMetadata>;

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
    #[method(name = "getTransactions")]
    async fn get_transactions(
        &self,
        /// the transaction query criteria.
        query: TransactionQuery,
        /// Optional paging cursor
        cursor: Option<TransactionDigest>,
        /// Maximum item returned per page
        limit: Option<usize>,
        /// query result ordering, default to false (ascending order), oldest record first.  
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionsPage>;

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
    ) -> RpcResult<GetPastObjectDataResponse>;

    /// Return the committee information for the asked epoch
    #[method(name = "getCommitteeInfo")]
    async fn get_committee_info(
        &self,
        /// The epoch of interest. If None, default to the latest epoch
        epoch: Option<EpochId>,
    ) -> RpcResult<CommitteeInfoResponse>;
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

    /// Send Coin<T> to a list of addresses, where `T` can be any coin type, following a list of amounts,
    /// The object specified in the `gas` field will be used to pay the gas fee for the transaction.
    /// The gas object can not appear in `input_coins`. If the gas object is not specified, the RPC server
    /// will auto-select one.
    #[method(name = "pay")]
    async fn pay(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// the Sui coins to be used in this transaction
        input_coins: Vec<ObjectID>,
        /// the recipients' addresses, the length of this vector must be the same as amounts.
        recipients: Vec<SuiAddress>,
        /// the amounts to be transferred to recipients, following the same order
        amounts: Vec<u64>,
        /// gas object to be used in this transaction, the gateway will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Send SUI coins to a list of addresses, following a list of amounts.
    /// This is for SUI coin only and does not require a separate gas coin object.
    /// Specifically, what pay_sui does are:
    /// 1. debit each input_coin to create new coin following the order of
    /// amounts and assign it to the corresponding recipient.
    /// 2. accumulate all residual SUI from input coins left and deposit all SUI to the first
    /// input coin, then use the first input coin as the gas coin object.
    /// 3. the balance of the first input coin after tx is sum(input_coins) - sum(amounts) - actual_gas_cost
    /// 4. all other input coints other than the first one are deleted.
    #[method(name = "paySui")]
    async fn pay_sui(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// the Sui coins to be used in this transaction, including the coin for gas payment.
        input_coins: Vec<ObjectID>,
        /// the recipients' addresses, the length of this vector must be the same as amounts.
        recipients: Vec<SuiAddress>,
        /// the amounts to be transferred to recipients, following the same order
        amounts: Vec<u64>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Send all SUI coins to one recipient.
    /// This is for SUI coin only and does not require a separate gas coin object.
    /// Specifically, what pay_all_sui does are:
    /// 1. accumulate all SUI from input coins and deposit all SUI to the first input coin
    /// 2. transfer the updated first coin to the recipient and also use this first coin as gas coin object.
    /// 3. the balance of the first input coin after tx is sum(input_coins) - actual_gas_cost.
    /// 4. all other input coins other than the first are deleted.
    #[method(name = "payAllSui")]
    async fn pay_all_sui(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// the Sui coins to be used in this transaction, including the coin for gas payment.
        input_coins: Vec<ObjectID>,
        /// the recipient address,
        recipient: SuiAddress,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: u64,
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

#[open_rpc(namespace = "sui", tag = "Transaction Subscription")]
#[rpc(server, client, namespace = "sui")]
pub trait TransactionStreamingApi {
    /// Subscribe to a stream of Sui event
    #[subscription(name = "subscribeTransaction", item = SuiTransactionResponse)]
    fn subscribe_transaction(
        &self,
        /// the filter criteria of the transaction stream.
        filter: SuiTransactionFilter,
    );
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
    /// Return list of events for a specified query criteria.
    #[method(name = "getEvents")]
    async fn get_events(
        &self,
        /// the event query criteria.
        query: EventQuery,
        /// optional paging cursor
        cursor: Option<EventID>,
        /// maximum number of items per page
        limit: Option<usize>,
        /// query result ordering, default to false (ascending order), oldest record first.  
        descending_order: Option<bool>,
    ) -> RpcResult<EventPage>;
}

#[open_rpc(namespace = "sui", tag = "APIs to execute transactions.")]
#[rpc(server, client, namespace = "sui")]
pub trait TransactionExecutionApi {
    /// Execute the transaction and wait for results if desired.
    /// Request types:
    /// 1. ImmediateReturn: immediately returns a response to client without waiting
    ///     for any execution results.  Note the transaction may fail without being
    ///     noticed by client in this mode. After getting the response, the client
    ///     may poll the node to check the result of the transaction.
    /// 2. WaitForTxCert: waits for TransactionCertificate and then return to client.
    /// 3. WaitForEffectsCert: waits for TransactionEffectsCert and then return to client.
    ///     This mode is a proxy for transaction finality.
    /// 4. WaitForLocalExecution: waits for TransactionEffectsCert and make sure the node
    ///     executed the transaction locally before returning the client. The local execution
    ///     makes sure this node is aware of this transaction when client fires subsequent queries.
    ///     However if the node fails to execute the transaction locally in a timely manner,
    ///     a bool type in the response is set to false to indicated the case.
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

#[open_rpc(
    namespace = "sui",
    tag = "Estimator API to estimate gas quantities for a transactions."
)]
#[rpc(server, client, namespace = "sui")]
pub trait EstimatorApi {
    /// Execute the transaction and wait for results if desired
    #[method(name = "estimateTransactionComputationCost")]
    async fn estimate_transaction_computation_cost(
        &self,
        /// transaction data bytes, as base-64 encoded string
        tx_bytes: Base64,
        computation_gas_unit_price: Option<u64>,
        storage_gas_unit_price: Option<u64>,
        mutated_object_sizes_after: Option<usize>,
        storage_rebate: Option<u64>,
    ) -> RpcResult<SuiGasCostSummary>;
}

pub fn cap_page_limit(limit: Option<usize>) -> Result<usize, anyhow::Error> {
    let limit = limit.unwrap_or(QUERY_MAX_RESULT_LIMIT);
    if limit == 0 {
        Err(anyhow!("Page result limit must be larger then 0."))?;
    }
    Ok(if limit > QUERY_MAX_RESULT_LIMIT {
        QUERY_MAX_RESULT_LIMIT
    } else {
        limit
    })
}
