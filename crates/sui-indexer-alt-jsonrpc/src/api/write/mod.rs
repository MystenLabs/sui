// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod response;

use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::types::ErrorObject;
use jsonrpsee::types::error::INTERNAL_ERROR_CODE;
use jsonrpsee::types::error::INVALID_PARAMS_CODE;
use prost_types::FieldMask;
use sui_indexer_alt_reader::fullnode_client::FullnodeClient;
use sui_json_rpc_types::DryRunTransactionBlockResponse;
use sui_json_rpc_types::SuiTransactionBlockData;
use sui_json_rpc_types::SuiTransactionBlockResponse;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::crypto::ToFromBytes;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionDataAPI;
use sui_types::transaction_driver_types::ExecuteTransactionRequestType;

use crate::api::rpc_module::RpcModule;
use crate::context::Context;
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
        if let Some(ExecuteTransactionRequestType::WaitForLocalExecution) = request_type {
            return Err(invalid_params(Error::DeprecatedWaitForLocalExecution).into());
        }

        let options = options.unwrap_or_default();
        let tx_data: TransactionData =
            bcs::from_bytes(&tx_bytes.to_vec().map_err(invalid_params_err)?).map_err(|e| {
                invalid_params_err(anyhow::anyhow!(
                    "Failed to deserialize TransactionData: {e}"
                ))
            })?;
        let tx_digest = tx_data.digest();

        let parsed_sigs = parse_signatures(&signatures)?;

        let read_mask = build_execute_read_mask(&options);

        let grpc_response = self
            .client
            .execute_transaction(tx_data.clone(), parsed_sigs.clone(), read_mask)
            .await
            .map_err(grpc_error_to_error_object)?;

        let executed_tx = grpc_response
            .transaction
            .as_ref()
            .ok_or_else(|| internal_err("Missing transaction in gRPC response"))?;

        let mut result = SuiTransactionBlockResponse::new(tx_digest);
        result.checkpoint = executed_tx.checkpoint;
        result.timestamp_ms = executed_tx
            .timestamp
            .and_then(|ts| sui_rpc::proto::proto_to_timestamp_ms(ts).ok());

        if options.show_input {
            result.transaction = Some(
                response::input(&self.context, tx_data.clone(), parsed_sigs.clone())
                    .await
                    .map_err(|e| {
                        internal_err(format!("Failed to convert transaction data: {e}"))
                    })?,
            );
        }

        if options.show_raw_input {
            result.raw_transaction = response::raw_input(&tx_data)
                .map_err(|e| internal_err(format!("Failed to serialize transaction: {e}")))?;
        }

        if options.show_raw_effects {
            result.raw_effects = response::raw_effects(executed_tx)
                .map_err(|e| internal_err(format!("Failed to extract raw effects: {e}")))?;
        }

        if options.show_effects || options.show_object_changes {
            let effects = response::deserialize_effects(executed_tx)
                .map_err(|e| internal_err(format!("Failed to deserialize effects: {e}")))?;

            if options.show_effects {
                result.effects = Some(
                    effects
                        .clone()
                        .try_into()
                        .map_err(|e| internal_err(format!("Failed to convert effects: {e}")))?,
                );
            }

            if options.show_object_changes {
                result.object_changes = Some(
                    response::object_changes(tx_data.sender(), &effects, executed_tx).map_err(
                        |e| internal_err(format!("Failed to build object changes: {e}")),
                    )?,
                );
            }
        }

        if options.show_events {
            result.events = Some(
                response::events(&self.context, tx_digest, executed_tx)
                    .await
                    .map_err(|e| internal_err(format!("Failed to build events: {e}")))?,
            );
        }

        if options.show_balance_changes {
            result.balance_changes = Some(
                response::balance_changes(executed_tx)
                    .map_err(|e| internal_err(format!("Failed to build balance changes: {e}")))?,
            );
        }

        Ok(result)
    }

    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<DryRunTransactionBlockResponse> {
        let raw_tx_bytes = tx_bytes.to_vec().map_err(invalid_params_err)?;
        let tx_data: TransactionData = bcs::from_bytes(&raw_tx_bytes).map_err(|e| {
            invalid_params_err(anyhow::anyhow!(
                "Failed to deserialize TransactionData: {e}"
            ))
        })?;

        let mut proto_tx = proto::Transaction::default();
        proto_tx.bcs =
            Some(proto::Bcs::serialize(&tx_data).map_err(|e| {
                internal_err(format!("Failed to serialize transaction for gRPC: {e}"))
            })?);

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
            .map_err(grpc_error_to_error_object)?;

        let executed_tx = grpc_response
            .transaction
            .as_ref()
            .ok_or_else(|| internal_err("Missing transaction in dry run gRPC response"))?;

        let effects = response::deserialize_effects(executed_tx)
            .map_err(|e| internal_err(format!("Failed to deserialize effects: {e}")))?;

        let sui_effects = effects
            .clone()
            .try_into()
            .map_err(|e| internal_err(format!("Failed to convert effects: {e}")))?;

        let tx_digest = tx_data.digest();
        let events = response::events(&self.context, tx_digest, executed_tx)
            .await
            .map_err(|e| internal_err(format!("Failed to build events: {e}")))?;

        let object_changes = response::object_changes(tx_data.sender(), &effects, executed_tx)
            .map_err(|e| internal_err(format!("Failed to build object changes: {e}")))?;

        let balance_changes = response::balance_changes(executed_tx)
            .map_err(|e| internal_err(format!("Failed to build balance changes: {e}")))?;

        let input = SuiTransactionBlockData::try_from_with_package_resolver(
            tx_data,
            self.context.package_resolver(),
        )
        .await
        .map_err(|e| internal_err(format!("Failed to convert transaction data: {e}")))?;

        Ok(DryRunTransactionBlockResponse {
            effects: sui_effects,
            events,
            object_changes,
            balance_changes,
            input,
            execution_error_source: None,
            suggested_gas_price: grpc_response.suggested_gas_price,
        })
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

fn parse_signatures(signatures: &[Base64]) -> RpcResult<Vec<GenericSignature>> {
    signatures
        .iter()
        .enumerate()
        .map(|(i, sig)| {
            let bytes = sig.to_vec().map_err(|e| {
                invalid_params_err(anyhow::anyhow!("Invalid base64 in signature {i}: {e}"))
            })?;
            GenericSignature::from_bytes(&bytes)
                .map_err(|e| invalid_params_err(anyhow::anyhow!("Invalid signature {i}: {e}")))
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

fn grpc_error_to_error_object(
    error: sui_indexer_alt_reader::fullnode_client::Error,
) -> ErrorObject<'static> {
    use sui_indexer_alt_reader::fullnode_client::Error;
    match error {
        Error::GrpcExecutionError(status)
            if matches!(
                status.code(),
                tonic::Code::InvalidArgument | tonic::Code::NotFound
            ) =>
        {
            ErrorObject::owned(
                INVALID_PARAMS_CODE,
                status.message().to_string(),
                None::<()>,
            )
        }
        Error::NotConfigured => {
            ErrorObject::owned(INTERNAL_ERROR_CODE, error.to_string(), None::<()>)
        }
        _ => ErrorObject::owned(INTERNAL_ERROR_CODE, error.to_string(), None::<()>),
    }
}

fn invalid_params_err(err: impl std::fmt::Display) -> ErrorObject<'static> {
    ErrorObject::owned(INVALID_PARAMS_CODE, "Invalid params", Some(err.to_string()))
}

fn internal_err(msg: impl Into<String>) -> ErrorObject<'static> {
    ErrorObject::owned(INTERNAL_ERROR_CODE, msg.into(), None::<()>)
}
