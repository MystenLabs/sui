// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod response;

use anyhow::Context as _;
use diesel::ExpressionMethods;
use diesel::JoinOnDsl;
use diesel::QueryDsl;
use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use prost_types::FieldMask;
use sui_indexer_alt_schema::schema::kv_epoch_starts;
use sui_indexer_alt_schema::schema::kv_protocol_configs;
use sui_json_rpc_types::DevInspectArgs;
use sui_json_rpc_types::DevInspectResults;
use sui_json_rpc_types::DryRunTransactionBlockResponse;
use sui_json_rpc_types::SuiTransactionBlockResponse;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::ToFromBytes;
use sui_types::signature::GenericSignature;
use sui_types::sui_serde::BigInt;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionKind;
use sui_types::transaction_driver_types::ExecuteTransactionRequestType;

use crate::api::rpc_module::RpcModule;
use crate::context::Context;
use crate::error::RpcError;
use crate::error::invalid_params;

#[open_rpc(namespace = "sui", tag = "Write API")]
#[rpc(server, client, namespace = "sui")]
pub trait WriteApi {
    /// Execute the transaction with options to show different information in the response. The only
    /// supported request type is `WaitForEffectsCert`: waits for TransactionEffectsCert and then
    /// return to client. `WaitForLocalExecution` mode has been deprecated.
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

    /// Runs the transaction in dev-inspect mode, which allows for nearly any transaction (or Move
    /// call) with any arguments. Detailed results are provided, including both the transaction
    /// effects and any return values.
    #[method(name = "devInspectTransactionBlock")]
    async fn dev_inspect_transaction_block(
        &self,
        sender_address: SuiAddress,
        /// BCS encoded TransactionKind (as opposed to TransactionData, which includes gasBudget and gasPrice).
        tx_bytes: Base64,
        /// Gas is not charged, but gas usage is still calculated. Default to use reference gas price.
        gas_price: Option<BigInt<u64>>,
        /// The epoch to perform the call. Will be set from the system state object if not provided.
        epoch: Option<BigInt<u64>>,
        /// Additional arguments including gas_budget, gas_objects, gas_sponsor and skip_checks.
        additional_args: Option<DevInspectArgs>,
    ) -> RpcResult<DevInspectResults>;

    /// Return transaction execution effects including the gas cost summary,
    /// while the effects are not committed to the chain.
    #[method(name = "dryRunTransactionBlock")]
    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<DryRunTransactionBlockResponse>;
}

pub(crate) struct Write {
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
    pub(crate) fn new(context: Context) -> Self {
        Self { context }
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

    async fn dev_inspect_transaction_block(
        &self,
        sender_address: SuiAddress,
        tx_bytes: Base64,
        gas_price: Option<BigInt<u64>>,
        // The legacy fullnode implementation also ignores this argument.
        _epoch: Option<BigInt<u64>>,
        additional_args: Option<DevInspectArgs>,
    ) -> RpcResult<DevInspectResults> {
        Ok(self
            .dev_inspect_transaction_block_impl(
                sender_address,
                tx_bytes,
                gas_price,
                additional_args,
            )
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
        let client = self.context.fullnode_client()?;
        if let Some(ExecuteTransactionRequestType::WaitForLocalExecution) = request_type {
            return Err(invalid_params(Error::DeprecatedWaitForLocalExecution));
        }

        let options = options.unwrap_or_default();
        let tx_data = parse_transaction_data(&tx_bytes)?;
        let parsed_sigs = parse_signatures_impl(&signatures)?;
        let read_mask = build_execute_read_mask(&options);

        let grpc_response = client
            .execute_transaction(tx_data.clone(), parsed_sigs.clone(), read_mask)
            .await
            .map_err(grpc_error_to_rpc_error)?;

        let executed_tx = grpc_response
            .transaction
            .as_ref()
            .context("Missing transaction in gRPC response")?;

        response::transaction(&self.context, tx_data, parsed_sigs, executed_tx, &options).await
    }

    async fn dev_inspect_transaction_block_impl(
        &self,
        sender_address: SuiAddress,
        tx_bytes: Base64,
        gas_price: Option<BigInt<u64>>,
        additional_args: Option<DevInspectArgs>,
    ) -> Result<DevInspectResults, RpcError<Error>> {
        let client = self.context.fullnode_client()?;

        let DevInspectArgs {
            gas_sponsor,
            gas_budget,
            gas_objects,
            skip_checks,
            show_raw_txn_data_and_effects,
        } = additional_args.unwrap_or_default();

        let skip_checks = skip_checks.unwrap_or(true);
        let show_raw_txn_data_and_effects = show_raw_txn_data_and_effects.unwrap_or(false);

        let kind = parse_transaction_kind(&tx_bytes)?;
        let (reference_gas_price, max_tx_gas) = gas_defaults(&self.context).await?;

        // Synthesize the full TransactionData the caller would have signed, the same way the legacy
        // fullnode implementation does. An empty gas payment is replaced by a mock gas coin on the
        // fullnode during simulation.
        let tx_data = TransactionData::new_with_gas_coins_allow_sponsor(
            kind,
            sender_address,
            gas_objects.unwrap_or_default(),
            gas_budget.map(|budget| *budget).unwrap_or(max_tx_gas),
            gas_price.map(|price| *price).unwrap_or(reference_gas_price),
            gas_sponsor.unwrap_or(sender_address),
        );

        // The raw transaction data reflects what the caller specified: the gas payment stays empty
        // here, and the mock gas coin the fullnode injects during simulation only shows up in the
        // effects (matching the legacy implementation, which captures these bytes before simulation
        // for the same reason).
        let raw_txn_data = if show_raw_txn_data_and_effects {
            bcs::to_bytes(&tx_data).context("Failed to serialize transaction data")?
        } else {
            vec![]
        };

        let mut proto_tx = proto::Transaction::default();
        proto_tx.bcs = Some(
            proto::Bcs::serialize(&tx_data).context("Failed to serialize transaction for gRPC")?,
        );

        let read_mask = FieldMask::from_paths([
            "transaction.effects.bcs",
            "transaction.events.bcs",
            "command_outputs",
        ]);

        // Sending BCS TransactionData makes the fullnode skip transaction resolution and gas
        // selection, and allows it to inject a mock gas coin -- the exact code path the legacy
        // devInspect implementation uses.
        let grpc_response = client
            .simulate_transaction(proto_tx, !skip_checks, false, read_mask)
            .await
            .map_err(grpc_error_to_rpc_error)?;

        let executed_tx = grpc_response
            .transaction
            .as_ref()
            .context("Missing transaction in dev inspect gRPC response")?;

        response::dev_inspect(
            &self.context,
            tx_data,
            executed_tx,
            &grpc_response.command_outputs,
            raw_txn_data,
            show_raw_txn_data_and_effects,
        )
        .await
    }

    async fn dry_run_transaction_block_impl(
        &self,
        tx_bytes: Base64,
    ) -> Result<DryRunTransactionBlockResponse, RpcError<Error>> {
        let client = self.context.fullnode_client()?;
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

        let grpc_response = client
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

/// Fetch the reference gas price of the latest epoch, and the maximum gas budget under the protocol
/// version that epoch started with. These are the defaults dev-inspect uses when the request does
/// not specify a gas price or budget.
///
/// The max budget is read from `kv_protocol_configs` (rather than this binary's own
/// `ProtocolConfig`) so that the RPC keeps working when the chain's protocol version is newer than
/// the binary.
async fn gas_defaults(ctx: &Context) -> Result<(u64, u64), RpcError<Error>> {
    use kv_epoch_starts::dsl as e;
    use kv_protocol_configs::dsl as p;

    let mut conn = ctx
        .pg_reader()
        .connect()
        .await
        .context("Failed to connect to the database")?;

    // The inner join skips epoch rows whose protocol configs have not been indexed yet, falling
    // back to the previous epoch's values for the duration of that (sub-second) window, rather than
    // failing the request.
    let (reference_gas_price, max_tx_gas): (i64, Option<String>) = conn
        .first(
            e::kv_epoch_starts
                .inner_join(p::kv_protocol_configs.on(p::protocol_version.eq(e::protocol_version)))
                .filter(p::config_name.eq("max_tx_gas"))
                .order(e::epoch.desc())
                .select((e::reference_gas_price, p::config_value)),
        )
        .await
        .context("Failed to fetch the latest epoch's gas parameters")?;

    let max_tx_gas: u64 = max_tx_gas
        .context("max_tx_gas is not set")?
        .parse()
        .context("Failed to parse max_tx_gas")?;

    Ok((reference_gas_price as u64, max_tx_gas))
}

fn parse_transaction_kind(tx_bytes: &Base64) -> Result<TransactionKind, RpcError<Error>> {
    let raw_tx_bytes = tx_bytes
        .to_vec()
        .map_err(|e| invalid_params(Error::InvalidTransactionBytes(e.to_string())))?;
    bcs::from_bytes(&raw_tx_bytes).map_err(|e| {
        invalid_params(Error::InvalidTransactionBytes(format!(
            "Failed to deserialize TransactionKind: {e}"
        )))
    })
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
        Error::Internal(err) => err.context("Write API gRPC request failed").into(),
        Error::GrpcExecutionError(status) => anyhow::Error::new(status)
            .context("Write API gRPC request failed")
            .into(),
    }
}
