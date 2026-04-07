// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod response;

use anyhow::Context as _;
use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use prost_types::FieldMask;
use sui_indexer_alt_reader::fullnode_client::FullnodeClient;
use sui_json_rpc_types::DryRunTransactionBlockResponse;
use sui_json_rpc_types::SuiTransactionBlockResponse;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::crypto::ToFromBytes;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;
use sui_types::transaction_driver_types::ExecuteTransactionRequestType;

use crate::api::rpc_module::RpcModule;
use crate::context::Context;
use crate::error::RpcError;
use crate::error::invalid_params;

#[open_rpc(namespace = "sui", tag = "Write API")]
#[rpc(server, client, namespace = "sui")]
pub trait WriteApi {
    /// Execute the transaction with options to show different information in the response.
    /// The only supported request type is `WaitForEffectsCert`: waits for TransactionEffectsCert and then return to client.
    /// `WaitForLocalExecution` mode has been deprecated.
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

    /// Return transaction execution effects including the gas cost summary,
    /// while the effects are not committed to the chain.
    #[method(name = "dryRunTransactionBlock")]
    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<DryRunTransactionBlockResponse>;
}

pub(crate) struct Write {
    client: FullnodeClient,
    context: Context,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("WaitForLocalExecution mode is deprecated")]
    DeprecatedWaitForLocalExecution,

    #[error("Invalid transaction bytes: {0}")]
    InvalidTransactionBytes(String),

    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("Transaction execution failed: {0}")]
    ExecutionFailed(String),
}

impl Write {
    pub(crate) fn new(client: FullnodeClient, context: Context) -> Self {
        Self { client, context }
    }
}

#[async_trait::async_trait]
impl WriteApiServer for Write {
    async fn execute_transaction_block(
        &self,
        tx_bytes: Base64,
        signatures: Vec<Base64>,
        options: Option<SuiTransactionBlockResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> RpcResult<SuiTransactionBlockResponse> {
        Ok(self
            .execute_transaction_block_impl(tx_bytes, signatures, options, request_type)
            .await?)
    }

    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<DryRunTransactionBlockResponse> {
        Ok(self.dry_run_transaction_block_impl(tx_bytes).await?)
    }
}

impl RpcModule for Write {
    fn schema(&self) -> Module {
        WriteApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}

impl Write {
    async fn execute_transaction_block_impl(
        &self,
        tx_bytes: Base64,
        signatures: Vec<Base64>,
        options: Option<SuiTransactionBlockResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> Result<SuiTransactionBlockResponse, RpcError<Error>> {
        if let Some(ExecuteTransactionRequestType::WaitForLocalExecution) = request_type {
            return Err(invalid_params(Error::DeprecatedWaitForLocalExecution));
        }

        let options = options.unwrap_or_default();
        let tx_data = parse_transaction_data(&tx_bytes)?;
        let parsed_sigs = parse_signatures_impl(&signatures)?;
        let read_mask = build_execute_read_mask(&options);

        let grpc_response = self
            .client
            .execute_transaction(tx_data.clone(), parsed_sigs.clone(), read_mask)
            .await
            .map_err(grpc_error_to_rpc_error)?;

        let executed_tx = grpc_response
            .transaction
            .as_ref()
            .context("Missing transaction in gRPC response")?;

        response::transaction(&self.context, tx_data, parsed_sigs, executed_tx, &options).await
    }

    async fn dry_run_transaction_block_impl(
        &self,
        tx_bytes: Base64,
    ) -> Result<DryRunTransactionBlockResponse, RpcError<Error>> {
        let tx_data = parse_transaction_data(&tx_bytes)?;

        let mut proto_tx = proto::Transaction::default();
        proto_tx.bcs = Some(
            proto::Bcs::serialize(&tx_data).context("Failed to serialize transaction for gRPC")?,
        );

        let read_mask = FieldMask::from_paths([
            "transaction.effects.bcs",
            "transaction.transaction.bcs",
            "transaction.events.bcs",
            "transaction.balance_changes",
            "transaction.effects.changed_objects",
            "transaction.objects.objects.bcs",
            "transaction.checkpoint",
            "transaction.timestamp",
            "suggested_gas_price",
        ]);

        let grpc_response = self
            .client
            .simulate_transaction(proto_tx, true, false, read_mask)
            .await
            .map_err(grpc_error_to_rpc_error)?;

        let executed_tx = grpc_response
            .transaction
            .as_ref()
            .context("Missing transaction in dry run gRPC response")?;

        response::dry_run(
            &self.context,
            tx_data,
            executed_tx,
            grpc_response.suggested_gas_price,
        )
        .await
    }
}

fn parse_transaction_data(tx_bytes: &Base64) -> Result<TransactionData, RpcError<Error>> {
    let raw_tx_bytes = tx_bytes
        .to_vec()
        .map_err(|e| invalid_params(Error::InvalidTransactionBytes(e.to_string())))?;
    bcs::from_bytes(&raw_tx_bytes).map_err(|e| {
        invalid_params(Error::InvalidTransactionBytes(format!(
            "Failed to deserialize TransactionData: {e}"
        )))
    })
}

fn parse_signatures_impl(signatures: &[Base64]) -> Result<Vec<GenericSignature>, RpcError<Error>> {
    signatures
        .iter()
        .enumerate()
        .map(|(i, sig)| {
            let bytes = sig.to_vec().map_err(|e| {
                invalid_params(Error::InvalidSignature(format!(
                    "Invalid base64 in signature {i}: {e}"
                )))
            })?;
            GenericSignature::from_bytes(&bytes).map_err(|e| {
                invalid_params(Error::InvalidSignature(format!(
                    "Invalid signature {i}: {e}"
                )))
            })
        })
        .collect()
}

fn build_execute_read_mask(options: &SuiTransactionBlockResponseOptions) -> FieldMask {
    let mut paths = vec!["checkpoint", "timestamp"];

    if options.show_effects || options.show_raw_effects || options.show_object_changes {
        paths.push("effects.bcs");
    }

    if options.show_object_changes {
        paths.push("effects.changed_objects");
        paths.push("objects.objects.bcs");
    }

    if options.show_events {
        paths.push("events.bcs");
    }

    if options.show_balance_changes {
        paths.push("balance_changes");
    }

    FieldMask::from_paths(paths)
}

fn grpc_error_to_rpc_error(
    error: sui_indexer_alt_reader::fullnode_client::Error,
) -> RpcError<Error> {
    use sui_indexer_alt_reader::fullnode_client::Error;
    match error {
        Error::GrpcExecutionError(status)
            if matches!(
                status.code(),
                tonic::Code::InvalidArgument | tonic::Code::NotFound
            ) =>
        {
            invalid_params(crate::api::write::Error::ExecutionFailed(
                status.message().to_string(),
            ))
        }
        Error::NotConfigured => anyhow::Error::new(Error::NotConfigured)
            .context("Fullnode client not configured for write API")
            .into(),
        Error::Internal(err) => err.context("Write API gRPC request failed").into(),
        Error::GrpcExecutionError(status) => anyhow::Error::new(status)
            .context("Write API gRPC request failed")
            .into(),
    }
}
