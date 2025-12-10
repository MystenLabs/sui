// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::Base64;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_json_rpc_types::{
    DryRunTransactionBlockResponse, SuiTransactionBlock, SuiTransactionBlockEffects,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    quorum_driver_types::ExecuteTransactionRequestType,
    transaction::{Transaction, TransactionData},
};

use sui_indexer_alt_jsonrpc::{api::rpc_module::RpcModule, error::invalid_params};
use sui_types::effects::TransactionEffectsAPI;

use crate::forking_store::ForkingStore;
use rand::rngs::OsRng;
use simulacrum::Simulacrum;
use std::sync::{Arc, RwLock};

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

pub(crate) struct Write(pub Arc<RwLock<Simulacrum<OsRng, ForkingStore>>>);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("WaitForLocalExecution mode is deprecated")]
    DeprecatedWaitForLocalExecution,
    #[error("Invalid base64: {0}")]
    InvalidBase64(String),
    #[error("Failed to decode transaction data: {0}")]
    DecodeError(String),
    #[error("Failed to execute transaction: {0}")]
    ExecutionError(String),
    #[error("Failed to convert: {0}")]
    ConversionError(String),
    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

impl Write {
    pub fn new(simulacrum: Arc<RwLock<Simulacrum<OsRng, ForkingStore>>>) -> Self {
        Self(simulacrum)
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

        let tx_data_decoded = tx_bytes
            .to_vec()
            .map_err(|e| invalid_params(Error::InvalidBase64(e.to_string())))?;
        let tx_data = bcs::from_bytes::<TransactionData>(&tx_data_decoded)
            .map_err(|e| invalid_params(Error::DecodeError(e.to_string())))?;

        let transaction = Transaction::from_data(tx_data, vec![]);

        // Execute the transaction using Simulacrum
        let mut simulacrum = self.0.write().unwrap();
        let (effects, _execution_error) = simulacrum
            .execute_transaction(transaction.clone())
            .map_err(|e| invalid_params(Error::ExecutionError(e.to_string())))?;

        // Build the response based on options
        let options = options.unwrap_or_default();
        let mut response = SuiTransactionBlockResponse::new(effects.transaction_digest().clone());

        if options.show_effects {
            response.effects = Some(
                SuiTransactionBlockEffects::try_from(effects.clone()).map_err(|e| {
                    invalid_params(Error::ConversionError(format!("effects: {}", e)))
                })?,
            );
        }

        if options.show_raw_input {
            response.raw_transaction = tx_bytes.to_vec().unwrap_or_default();
        }

        // TODO: Implement other options (events, object_changes, balance_changes) as needed

        Ok(response)
    }

    async fn dry_run_transaction_block(
        &self,
        _tx_bytes: Base64,
    ) -> RpcResult<DryRunTransactionBlockResponse> {
        Err(invalid_params(Error::NotImplemented("Dry run".to_string())).into())
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
