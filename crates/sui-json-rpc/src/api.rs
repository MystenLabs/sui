// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;

use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    Balance, CoinPage, DevInspectResults, DynamicFieldPage, EventPage, GetObjectDataResponse,
    GetPastObjectDataResponse, GetRawObjectDataResponse, MoveFunctionArgType,
    RPCTransactionRequestParams, SuiCoinMetadata, SuiEventEnvelope, SuiEventFilter,
    SuiExecuteTransactionResponse, SuiMoveNormalizedFunction, SuiMoveNormalizedModule,
    SuiMoveNormalizedStruct, SuiObjectInfo, SuiTBlsSignObjectCommitmentType,
    SuiTBlsSignRandomnessObjectResponse, SuiTransactionAuthSignersResponse,
    SuiTransactionBuilderMode, SuiTransactionEffects, SuiTransactionFilter, SuiTransactionResponse,
    SuiTypeTag, TransactionBytes, TransactionsPage,
};
use sui_open_rpc_macros::open_rpc;
use sui_types::balance::Supply;
use sui_types::base_types::{
    ObjectID, SequenceNumber, SuiAddress, TransactionDigest, TxSequenceNumber,
};
use sui_types::committee::EpochId;
use sui_types::event::EventID;
use sui_types::governance::DelegatedStake;
use sui_types::messages::CommitteeInfoResponse;
use sui_types::messages::ExecuteTransactionRequestType;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointContentsDigest, CheckpointDigest, CheckpointSequenceNumber,
    CheckpointSummary,
};
use sui_types::query::{EventQuery, TransactionQuery};
use sui_types::sui_system_state::{SuiSystemState, ValidatorMetadata};

/// Maximum number of events returned in an event query.
/// This is equivalent to EVENT_QUERY_MAX_LIMIT in `sui-storage` crate.
/// To avoid unnecessary dependency on that crate, we have a reference here
/// for document purposes.
pub const QUERY_MAX_RESULT_LIMIT: usize = 1000;

#[open_rpc(namespace = "sui", tag = "Coin Query API")]
#[rpc(server, client, namespace = "sui")]
pub trait CoinReadApi {
    /// Return all Coin<`coin_type`> objects owned by an address.
    #[method(name = "getCoins")]
    async fn get_coins(
        &self,
        /// the owner's Sui address
        owner: SuiAddress,
        /// optional fully qualified type names for the coin (e.g., 0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC), default to 0x2::sui::SUI if not specified.
        coin_type: Option<String>,
        /// optional paging cursor
        cursor: Option<ObjectID>,
        /// maximum number of items per page
        limit: Option<usize>,
    ) -> RpcResult<CoinPage>;

    /// Return all Coin objects owned by an address.
    #[method(name = "getAllCoins")]
    async fn get_all_coins(
        &self,
        /// the owner's Sui address
        owner: SuiAddress,
        /// optional paging cursor
        cursor: Option<ObjectID>,
        /// maximum number of items per page
        limit: Option<usize>,
    ) -> RpcResult<CoinPage>;

    /// Return the total coin balance for one coin type, owned by the address owner.
    #[method(name = "getBalance")]
    async fn get_balance(
        &self,
        /// the owner's Sui address
        owner: SuiAddress,
        /// optional fully qualified type names for the coin (e.g., 0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC), default to 0x2::sui::SUI if not specified.
        coin_type: Option<String>,
    ) -> RpcResult<Balance>;

    /// Return the total coin balance for all coin type, owned by the address owner.
    #[method(name = "getAllBalances")]
    async fn get_all_balances(
        &self,
        /// the owner's Sui address
        owner: SuiAddress,
    ) -> RpcResult<Vec<Balance>>;

    /// Return metadata(e.g., symbol, decimals) for a coin
    #[method(name = "getCoinMetadata")]
    async fn get_coin_metadata(
        &self,
        /// fully qualified type names for the coin (e.g., 0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC)
        coin_type: String,
    ) -> RpcResult<SuiCoinMetadata>;

    /// Return total supply for a coin
    #[method(name = "getTotalSupply")]
    async fn get_total_supply(
        &self,
        /// fully qualified type names for the coin (e.g., 0x168da5bf1f48dafc111b0a488fa454aca95e0b5e::usdc::USDC)
        coin_type: String,
    ) -> RpcResult<Supply>;
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

    /// Return the authority public keys that commits to the authority signature of the transaction.
    #[method(name = "getTransactionAuthSigners")]
    async fn get_transaction_auth_signers(
        &self,
        /// the digest of the queried transaction
        digest: TransactionDigest,
    ) -> RpcResult<SuiTransactionAuthSignersResponse>;

    /// Return the object information for a specified object
    #[method(name = "getObject")]
    async fn get_object(
        &self,
        /// the ID of the queried object
        object_id: ObjectID,
    ) -> RpcResult<GetObjectDataResponse>;

    /// Return the dynamic field object information for a specified object
    #[method(name = "getDynamicFieldObject")]
    async fn get_dynamic_field_object(
        &self,
        /// The ID of the queried parent object
        parent_object_id: ObjectID,
        /// The Name of the dynamic field
        name: String,
    ) -> RpcResult<GetObjectDataResponse>;
}

#[open_rpc(namespace = "sui", tag = "Full Node API")]
#[rpc(server, client, namespace = "sui")]
pub trait RpcFullNodeReadApi {
    /// Return dev-inpsect results of the transaction, including both the transaction
    /// effects and return values of the transaction.
    #[method(name = "devInspectTransaction")]
    async fn dev_inspect_transaction(
        &self,
        tx_bytes: Base64,
        /// The epoch to perform the call. Will be set from the system state object if not provided
        epoch: Option<EpochId>,
    ) -> RpcResult<DevInspectResults>;

    /// Similar to `dev_inspect_transaction` but do not require gas object and budget
    #[method(name = "devInspectMoveCall")]
    async fn dev_inspect_move_call(
        &self,
        /// the caller's Sui address
        sender_address: SuiAddress,
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
        /// The epoch to perform the call. Will be set from the system state object if not provided
        epoch: Option<EpochId>,
    ) -> RpcResult<DevInspectResults>;

    /// Return transaction execution effects including the gas cost summary,
    /// while the effects are not committed to the chain.
    #[method(name = "dryRunTransaction")]
    async fn dry_run_transaction(&self, tx_bytes: Base64) -> RpcResult<SuiTransactionEffects>;

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
        /// Maximum item returned per page, default to [QUERY_MAX_RESULT_LIMIT] if not specified.
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

    /// Return the sequence number of the latest checkpoint that has been executed
    #[method(name = "getLatestCheckpointSequenceNumber")]
    fn get_latest_checkpoint_sequence_number(&self) -> RpcResult<CheckpointSequenceNumber>;

    /// Return a checkpoint summary based on a checkpoint sequence number
    #[method(name = "getCheckpointSummary")]
    fn get_checkpoint_summary(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> RpcResult<CheckpointSummary>;

    /// Return a checkpoint summary based on checkpoint digest
    #[method(name = "getCheckpointSummaryByDigest")]
    fn get_checkpoint_summary_by_digest(
        &self,
        digest: CheckpointDigest,
    ) -> RpcResult<CheckpointSummary>;

    /// Return contents of a checkpoint, namely a list of execution digests
    #[method(name = "getCheckpointContents")]
    fn get_checkpoint_contents(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> RpcResult<CheckpointContents>;

    /// Return contents of a checkpoint based on checkpoint content digest
    #[method(name = "getCheckpointContentsByDigest")]
    fn get_checkpoint_contents_by_digest(
        &self,
        digest: CheckpointContentsDigest,
    ) -> RpcResult<CheckpointContents>;
}

#[open_rpc(namespace = "sui", tag = "Governance Read API")]
#[rpc(server, client, namespace = "sui")]
pub trait GovernanceReadApi {
    /// Return all [DelegatedStake].
    #[method(name = "getDelegatedStakes")]
    async fn get_delegated_stakes(&self, owner: SuiAddress) -> RpcResult<Vec<DelegatedStake>>;

    /// Return all validators available for stake delegation.
    #[method(name = "getValidators")]
    async fn get_validators(&self) -> RpcResult<Vec<ValidatorMetadata>>;

    /// Return the committee information for the asked `epoch`.
    #[method(name = "getCommitteeInfo")]
    async fn get_committee_info(
        &self,
        /// The epoch of interest. If None, default to the latest epoch
        epoch: Option<EpochId>,
    ) -> RpcResult<CommitteeInfoResponse>;

    /// Return [SuiSystemState]
    #[method(name = "getSuiSystemState")]
    async fn get_sui_system_state(&self) -> RpcResult<SuiSystemState>;

    /// Return the reference gas price for the network
    #[method(name = "getReferenceGasPrice")]
    async fn get_reference_gas_price(&self) -> RpcResult<u64>;
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
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
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
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
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
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: u64,
        /// Whether this is a Normal transaction or a Dev Inspect Transaction. Default to be `SuiTransactionBuilderMode::Commit` when it's None.
        execution_mode: Option<SuiTransactionBuilderMode>,
    ) -> RpcResult<TransactionBytes>;

    /// Create an unsigned transaction to publish Move module.
    #[method(name = "publish")]
    async fn publish(
        &self,
        /// the transaction signer's Sui address
        sender: SuiAddress,
        /// the compiled bytes of a move module, the
        compiled_modules: Vec<Base64>,
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
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
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
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
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
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
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
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
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: u64,
        /// Whether this is a regular transaction or a Dev Inspect Transaction
        txn_builder_mode: Option<SuiTransactionBuilderMode>,
    ) -> RpcResult<TransactionBytes>;

    /// Add delegated stake to a validator's staking pool using multiple coins and amount.
    #[method(name = "requestAddDelegation")]
    async fn request_add_delegation(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// Coin<SUI> or LockedCoin<SUI> object to delegate
        coins: Vec<ObjectID>,
        /// delegation amount
        amount: Option<u64>,
        /// the validator's Sui address
        validator: SuiAddress,
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Withdraw a delegation from a validator's staking pool.
    #[method(name = "requestWithdrawDelegation")]
    async fn request_withdraw_delegation(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// Delegation object ID
        delegation: ObjectID,
        /// StakedSui object ID
        staked_sui: ObjectID,
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Switch delegation from the current validator to a new one.
    #[method(name = "requestSwitchDelegation")]
    async fn request_switch_delegation(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// Delegation object ID
        delegation: ObjectID,
        /// StakedSui object ID
        staked_sui: ObjectID,
        /// Validator to switch to
        new_validator_address: SuiAddress,
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
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
        /// maximum number of items per page, default to [QUERY_MAX_RESULT_LIMIT] if not specified.
        limit: Option<usize>,
        /// query result ordering, default to false (ascending order), oldest record first.
        descending_order: Option<bool>,
    ) -> RpcResult<EventPage>;
}

#[open_rpc(namespace = "sui", tag = "Threshold BLS APIs")]
#[rpc(server, client, namespace = "sui")]
pub trait ThresholdBlsApi {
    /// Sign an a Randomness object with threshold BLS.
    #[method(name = "tblsSignRandomnessObject")]
    async fn tbls_sign_randomness_object(
        &self,
        /// The object ID.
        object_id: ObjectID,
        /// The way in which the commitment on the object creation should be verified.
        commitment_type: SuiTBlsSignObjectCommitmentType,
    ) -> RpcResult<SuiTBlsSignRandomnessObjectResponse>;
}

#[open_rpc(namespace = "sui", tag = "APIs to execute transactions.")]
#[rpc(server, client, namespace = "sui")]
pub trait TransactionExecutionApi {
    /// Execute the transaction and wait for results if desired.
    /// Request types:
    /// 1. WaitForEffectsCert: waits for TransactionEffectsCert and then return to client.
    ///     This mode is a proxy for transaction finality.
    /// 2. WaitForLocalExecution: waits for TransactionEffectsCert and make sure the node
    ///     executed the transaction locally before returning the client. The local execution
    ///     makes sure this node is aware of this transaction when client fires subsequent queries.
    ///     However if the node fails to execute the transaction locally in a timely manner,
    ///     a bool type in the response is set to false to indicated the case.
    // TODO(joyqvq): remove this and rename executeTransactionSerializedSig to executeTransaction
    #[method(name = "executeTransaction")]
    async fn execute_transaction(
        &self,
        /// BCS serialized transaction data bytes without its type tag, as base-64 encoded string.
        tx_bytes: Base64,
        /// `flag || signature || pubkey` bytes, as base-64 encoded string, signature is committed to the intent message of the transaction data, as base-64 encoded string.
        signature: Base64,
        /// The request type
        request_type: ExecuteTransactionRequestType,
    ) -> RpcResult<SuiExecuteTransactionResponse>;

    #[method(name = "executeTransactionSerializedSig")]
    async fn execute_transaction_serialized_sig(
        &self,
        /// BCS serialized transaction data bytes without its type tag, as base-64 encoded string.
        tx_bytes: Base64,
        /// `flag || signature || pubkey` bytes, as base-64 encoded string, signature is committed to the intent message of the transaction data, as base-64 encoded string.
        signature: Base64,
        /// The request type
        request_type: ExecuteTransactionRequestType,
    ) -> RpcResult<SuiExecuteTransactionResponse>;
}

pub fn cap_page_limit(limit: Option<usize>) -> usize {
    let limit = limit.unwrap_or_default();
    if limit > QUERY_MAX_RESULT_LIMIT || limit == 0 {
        QUERY_MAX_RESULT_LIMIT
    } else {
        limit
    }
}
