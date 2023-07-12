// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;

use sui_json_rpc_types::{
    DevInspectResults, DryRunTransactionBlockResponse, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::SuiAddress;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::sui_serde::BigInt;

#[open_rpc(namespace = "sui", tag = "Write API")]
#[rpc(server, client, namespace = "sui")]
pub trait WriteApi {
    /// Execute the transaction and wait for results if desired.
    /// Request types:
    /// 1. WaitForEffectsCert: waits for TransactionEffectsCert and then return to client.
    ///     This mode is a proxy for transaction finality.
    /// 2. WaitForLocalExecution: waits for TransactionEffectsCert and make sure the node
    ///     executed the transaction locally before returning the client. The local execution
    ///     makes sure this node is aware of this transaction when client fires subsequent queries.
    ///     However if the node fails to execute the transaction locally in a timely manner,
    ///     a bool type in the response is set to false to indicated the case.
    /// request_type is default to be `WaitForEffectsCert` unless options.show_events or options.show_effects is true
    #[method(name = "executeTransactionBlock")]
    async fn execute_transaction_block(
        &self,
        /// BCS serialized transaction data bytes without its type tag, as base-64 encoded string.
        tx_bytes: Base64,
        /// A list of signatures (`flag || signature || pubkey` bytes, as base-64 encoded string). Signature is committed to the intent message of the transaction data, as base-64 encoded string.
        signatures: Vec<Base64>,
        /// options for specifying the content to be returned
        options: Option<SuiTransactionBlockResponseOptions>,
        /// The request type, derived from `SuiTransactionBlockResponseOptions` if None
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> RpcResult<SuiTransactionBlockResponse>;

    /// Simulates running the transaction, even one that would be normally
    /// impossible, against the current state of the network. Nearly any
    /// transaction (or Move call) with any arguments can run in dev-inspect;
    /// transaction inputs are not checked, entry function verification is
    /// skipped, values can be left unused, and returns are not validated.
    /// Detailed results are provided, including both the transaction effects
    /// and any return values. Dev-inspect is provided to aid in understanding
    /// the effects of any possible action against the current network state.
    /// See dryRunTransactionBlock for a more faithful simulation of what would
    /// happen if the transaction were actually run.
    #[method(name = "devInspectTransactionBlock")]
    async fn dev_inspect_transaction_block(
        &self,
        sender_address: SuiAddress,
        /// BCS encoded TransactionKind(as opposed to TransactionData, which include gasBudget and gasPrice)
        tx_bytes: Base64,
        /// Gas is not charged, but gas usage is still calculated. Default to use reference gas price
        gas_price: Option<BigInt<u64>>,
        /// The epoch to perform the call. Will be set from the system state object if not provided
        epoch: Option<BigInt<u64>>,
    ) -> RpcResult<DevInspectResults>;

    /// Simulate running the transaction, including all normally applied
    /// checks, without actually running it. This is useful for estimating
    /// the gas fees of a transaction before running it, but can also be used
    /// to determine the side-effects of a transaction without actually running
    /// it.
    #[method(name = "dryRunTransactionBlock")]
    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<DryRunTransactionBlockResponse>;
}
