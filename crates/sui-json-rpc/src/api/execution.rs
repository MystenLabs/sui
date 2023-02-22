// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;

use sui_json_rpc_types::SuiExecuteTransactionResponse;

use sui_open_rpc_macros::open_rpc;
use sui_types::messages::ExecuteTransactionRequestType;

#[open_rpc(namespace = "sui", tag = "Transaction Execution API")]
#[rpc(server, client, namespace = "sui")]
pub trait TransactionExecution {
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
    #[method(name = "executeTransaction", deprecated)]
    async fn execute_transaction(
        &self,
        /// BCS serialized transaction data bytes without its type tag, as base-64 encoded string.
        tx_bytes: Base64,
        /// `flag || signature || pubkey` bytes, as base-64 encoded string, signature is committed to the intent message of the transaction data, as base-64 encoded string.
        signature: Base64,
        /// The request type
        request_type: ExecuteTransactionRequestType,
    ) -> RpcResult<SuiExecuteTransactionResponse>;

    #[method(name = "executeTransactionSerializedSig", deprecated)]
    async fn execute_transaction_serialized_sig(
        &self,
        /// BCS serialized transaction data bytes without its type tag, as base-64 encoded string.
        tx_bytes: Base64,
        /// `flag || signature || pubkey` bytes, as base-64 encoded string, signature is committed to the intent message of the transaction data, as base-64 encoded string.
        signature: Base64,
        /// The request type
        request_type: ExecuteTransactionRequestType,
    ) -> RpcResult<SuiExecuteTransactionResponse>;

    // TODO: migrate above two rpc calls to this one eventually.
    #[method(name = "submitTransaction")]
    async fn submit_transaction(
        &self,
        /// BCS serialized transaction data bytes without its type tag, as base-64 encoded string.
        tx_bytes: Base64,
        /// A list of signatures (`flag || signature || pubkey` bytes, as base-64 encoded string). Signature is committed to the intent message of the transaction data, as base-64 encoded string.
        signatures: Vec<Base64>,
        /// The request type
        request_type: ExecuteTransactionRequestType,
    ) -> RpcResult<SuiExecuteTransactionResponse>;
}
