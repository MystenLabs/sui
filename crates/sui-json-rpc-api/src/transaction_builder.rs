// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;

use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    RPCTransactionRequestParams, SuiTransactionBlockBuilderMode, SuiTypeTag, TransactionBlockBytes,
};
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::sui_serde::BigInt;

#[open_rpc(namespace = "unsafe", tag = "Transaction Builder API")]
#[rpc(server, client, namespace = "unsafe")]
pub trait TransactionBuilder {
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
        gas_budget: BigInt<u64>,
        /// the recipient's Sui address
        recipient: SuiAddress,
    ) -> RpcResult<TransactionBlockBytes>;

    /// Create an unsigned transaction to send SUI coin object to a Sui address. The SUI object is also used as the gas object.
    #[method(name = "transferSui")]
    async fn transfer_sui(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// the Sui coin object to be used in this transaction
        sui_object_id: ObjectID,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: BigInt<u64>,
        /// the recipient's Sui address
        recipient: SuiAddress,
        /// the amount to be split out and transferred
        amount: Option<BigInt<u64>>,
    ) -> RpcResult<TransactionBlockBytes>;

    /// Send `Coin<T>` to a list of addresses, where `T` can be any coin type, following a list of amounts,
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
        amounts: Vec<BigInt<u64>>,
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes>;

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
        amounts: Vec<BigInt<u64>>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes>;

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
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes>;

    /// Create an unsigned transaction to execute a Move call on the network, by calling the specified function in the module of a given package.
    #[method(name = "moveCall")]
    async fn move_call(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// the Move package ID, e.g. `0x2`
        package_object_id: ObjectID,
        /// the Move module name, e.g. `pay`
        module: String,
        /// the move function name, e.g. `split`
        function: String,
        /// the type arguments of the Move function
        type_arguments: Vec<SuiTypeTag>,
        /// the arguments to be passed into the Move function, in [SuiJson](https://docs.sui.io/build/sui-json) format
        arguments: Vec<SuiJsonValue>,
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: BigInt<u64>,
        /// Whether this is a Normal transaction or a Dev Inspect Transaction. Default to be `SuiTransactionBlockBuilderMode::Commit` when it's None.
        execution_mode: Option<SuiTransactionBlockBuilderMode>,
    ) -> RpcResult<TransactionBlockBytes>;

    /// Create an unsigned transaction to publish a Move package.
    #[method(name = "publish")]
    async fn publish(
        &self,
        /// the transaction signer's Sui address
        sender: SuiAddress,
        /// the compiled bytes of a Move package
        compiled_modules: Vec<Base64>,
        /// a list of transitive dependency addresses that this set of modules depends on.
        dependencies: Vec<ObjectID>,
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes>;

    /// Create an unsigned transaction to split a coin object into multiple coins.
    #[method(name = "splitCoin")]
    async fn split_coin(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// the coin object to be spilt
        coin_object_id: ObjectID,
        /// the amounts to split out from the coin
        split_amounts: Vec<BigInt<u64>>,
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes>;

    /// Create an unsigned transaction to split a coin object into multiple equal-size coins.
    #[method(name = "splitCoinEqual")]
    async fn split_coin_equal(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// the coin object to be spilt
        coin_object_id: ObjectID,
        /// the number of coins to split into
        split_count: BigInt<u64>,
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes>;

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
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes>;

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
        gas_budget: BigInt<u64>,
        /// Whether this is a regular transaction or a Dev Inspect Transaction
        txn_builder_mode: Option<SuiTransactionBlockBuilderMode>,
    ) -> RpcResult<TransactionBlockBytes>;

    /// Add stake to a validator's staking pool using multiple coins and amount.
    #[method(name = "requestAddStake")]
    async fn request_add_stake(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// Coin<SUI> object to stake
        coins: Vec<ObjectID>,
        /// stake amount
        amount: Option<BigInt<u64>>,
        /// the validator's Sui address
        validator: SuiAddress,
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes>;

    /// Withdraw stake from a validator's staking pool.
    #[method(name = "requestWithdrawStake")]
    async fn request_withdraw_stake(
        &self,
        /// the transaction signer's Sui address
        signer: SuiAddress,
        /// StakedSui object ID
        staked_sui: ObjectID,
        /// gas object to be used in this transaction, node will pick one from the signer's possession if not provided
        gas: Option<ObjectID>,
        /// the gas budget, the transaction will fail if the gas cost exceed the budget
        gas_budget: BigInt<u64>,
    ) -> RpcResult<TransactionBlockBytes>;
}
