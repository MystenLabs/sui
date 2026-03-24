// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Minimal JSON-RPC server for sui-forking.
//!
//! Implements the subset of Sui JSON-RPC methods needed by the Sui SDK client
//! (`SuiClientBuilder::build` requires `rpc.discover`) and the walrus SDK
//! (transaction execution, gas price, object reads).
//!
//! The heavy lifting is delegated to the shared `execution` module and the
//! simulacrum — this module only handles JSON-RPC serialization.

use axum::Router;
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::types::ErrorObjectOwned;
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiObjectResponse, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_types::{
    base_types::ObjectID, digests::TransactionDigest, effects::TransactionEffectsAPI,
    error::SuiObjectResponseError, object::ObjectRead, transaction::TransactionData,
    transaction_driver_types::ExecuteTransactionRequestType,
};
use tracing::info;

use crate::context::Context;

/// JSON-RPC API trait for the forking server.
///
/// Only the methods needed by the Sui SDK and walrus SDK are implemented.
/// `rpc.discover` is provided automatically by jsonrpsee.
#[rpc(server)]
pub trait ForkingApi {
    #[method(name = "sui_executeTransactionBlock")]
    async fn execute_transaction_block(
        &self,
        tx_bytes: String,
        signatures: Vec<String>,
        options: Option<SuiTransactionBlockResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> Result<SuiTransactionBlockResponse, ErrorObjectOwned>;

    #[method(name = "suix_getReferenceGasPrice")]
    async fn get_reference_gas_price(&self) -> Result<String, ErrorObjectOwned>;

    #[method(name = "sui_getObject")]
    async fn get_object(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> Result<SuiObjectResponse, ErrorObjectOwned>;

    #[method(name = "sui_multiGetObjects")]
    async fn multi_get_objects(
        &self,
        object_ids: Vec<ObjectID>,
        options: Option<SuiObjectDataOptions>,
    ) -> Result<Vec<SuiObjectResponse>, ErrorObjectOwned>;

    #[method(name = "sui_getTransactionBlock")]
    async fn get_transaction_block(
        &self,
        digest: TransactionDigest,
        options: Option<SuiTransactionBlockResponseOptions>,
    ) -> Result<SuiTransactionBlockResponse, ErrorObjectOwned>;

    #[method(name = "sui_getChainIdentifier")]
    async fn get_chain_identifier(&self) -> Result<String, ErrorObjectOwned>;
}

struct ForkingApiImpl {
    context: Context,
}

fn internal_error(msg: impl Into<String>) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32603, msg.into(), None::<()>)
}

#[async_trait]
impl ForkingApiServer for ForkingApiImpl {
    async fn execute_transaction_block(
        &self,
        tx_bytes: String,
        _signatures: Vec<String>,
        options: Option<SuiTransactionBlockResponseOptions>,
        _request_type: Option<ExecuteTransactionRequestType>,
    ) -> Result<SuiTransactionBlockResponse, ErrorObjectOwned> {
        use base64::Engine;

        let tx_data_bytes = base64::engine::general_purpose::STANDARD
            .decode(&tx_bytes)
            .map_err(|e| internal_error(format!("Invalid base64 tx_bytes: {e}")))?;

        let tx_data: TransactionData = bcs::from_bytes(&tx_data_bytes)
            .map_err(|e| internal_error(format!("Invalid BCS transaction data: {e}")))?;

        info!("JSON-RPC: executing transaction");

        let result = crate::execution::execute_transaction(&self.context, tx_data)
            .await
            .map_err(|e| internal_error(format!("Transaction execution failed: {e:?}")))?;

        let options = options.unwrap_or_default();
        let mut response = SuiTransactionBlockResponse {
            digest: *result.effects.transaction_digest(),
            ..Default::default()
        };

        if options.show_effects {
            response.effects = Some(result.effects.try_into().map_err(
                |e: sui_types::error::SuiError| {
                    internal_error(format!("Failed to convert effects: {e}"))
                },
            )?);
        }

        Ok(response)
    }

    async fn get_reference_gas_price(&self) -> Result<String, ErrorObjectOwned> {
        let sim = self.context.simulacrum.read().await;
        Ok(sim.reference_gas_price().to_string())
    }

    async fn get_object(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> Result<SuiObjectResponse, ErrorObjectOwned> {
        let mut options = options.unwrap_or_default();
        // We don't have Move layout resolution, so disable content/display
        // which require it. BCS, type, and owner still work without layout.
        options.show_content = false;
        options.show_display = false;

        let sim = self.context.simulacrum.read().await;
        let store = sim.store();

        let object: Option<sui_types::object::Object> =
            sui_types::storage::ObjectStore::get_object(store, &object_id);

        match object {
            Some(obj) => {
                let object_ref = obj.compute_object_reference();
                let object_read = ObjectRead::Exists(object_ref, obj, None);
                SuiObjectResponse::try_from((object_read, options))
                    .map_err(|e| internal_error(format!("Failed to convert object: {e}")))
            }
            None => Ok(SuiObjectResponse::new_with_error(
                SuiObjectResponseError::NotExists { object_id },
            )),
        }
    }

    async fn multi_get_objects(
        &self,
        object_ids: Vec<ObjectID>,
        options: Option<SuiObjectDataOptions>,
    ) -> Result<Vec<SuiObjectResponse>, ErrorObjectOwned> {
        let mut options = options.unwrap_or_default();
        options.show_content = false;
        options.show_display = false;
        let sim = self.context.simulacrum.read().await;
        let store = sim.store();

        object_ids
            .into_iter()
            .map(|object_id| {
                let object: Option<sui_types::object::Object> =
                    sui_types::storage::ObjectStore::get_object(store, &object_id);
                match object {
                    Some(obj) => {
                        let object_ref = obj.compute_object_reference();
                        let object_read = ObjectRead::Exists(object_ref, obj, None);
                        SuiObjectResponse::try_from((object_read, options.clone()))
                            .map_err(|e| internal_error(format!("Failed to convert object: {e}")))
                    }
                    None => Ok(SuiObjectResponse::new_with_error(
                        SuiObjectResponseError::NotExists { object_id },
                    )),
                }
            })
            .collect()
    }

    async fn get_transaction_block(
        &self,
        digest: TransactionDigest,
        options: Option<SuiTransactionBlockResponseOptions>,
    ) -> Result<SuiTransactionBlockResponse, ErrorObjectOwned> {
        let options = options.unwrap_or_default();
        let sim = self.context.simulacrum.read().await;
        let store = sim.store();

        let effects = store
            .get_transaction_effects(&digest)
            .ok_or_else(|| internal_error(format!("Transaction {digest} not found")))?;

        let mut response = SuiTransactionBlockResponse {
            digest,
            ..Default::default()
        };

        if options.show_effects {
            response.effects = Some(effects.clone().try_into().map_err(
                |e: sui_types::error::SuiError| {
                    internal_error(format!("Failed to convert effects: {e}"))
                },
            )?);
        }

        // The SDK polls until checkpoint is Some. In sui-forking, execute_transaction
        // creates a checkpoint immediately, so if effects exist the tx is checkpointed.
        if let Some(cp) = store.get_highest_checkpint() {
            response.checkpoint = Some(cp.sequence_number);
        }

        Ok(response)
    }

    async fn get_chain_identifier(&self) -> Result<String, ErrorObjectOwned> {
        Ok(self.context.chain_id.to_string())
    }
}

/// Creates an axum Router that serves JSON-RPC on POST `/`.
pub fn create_jsonrpc_router(context: Context) -> anyhow::Result<Router> {
    use jsonrpsee::RpcModule;

    let server = ForkingApiImpl { context };
    let mut module = RpcModule::new(());
    module
        .merge(server.into_rpc())
        .map_err(|e| anyhow::anyhow!("Failed to merge JSON-RPC module: {e}"))?;

    // Register rpc.discover — required by SuiClientBuilder::build() to create a SuiClient.
    // Returns a minimal OpenRPC spec with the version and registered method names.
    let method_names: Vec<serde_json::Value> = module
        .method_names()
        .map(|name| serde_json::json!({ "name": name }))
        .collect();
    module
        .register_method("rpc.discover", move |_, _, _| {
            serde_json::json!({
                "info": {
                    "title": "Sui Forking JSON-RPC",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "methods": method_names,
            })
        })
        .map_err(|e| anyhow::anyhow!("Failed to register rpc.discover: {e}"))?;

    let (stop_handle, _server_handle) = jsonrpsee::server::stop_channel();

    let service = jsonrpsee::server::ServerBuilder::new()
        .http_only()
        .to_service_builder()
        .build(module, stop_handle);

    // Use HandleError to convert the jsonrpsee service error type (BoxError)
    // to an axum-compatible IntoResponse, then use it as a fallback.
    let svc = axum::error_handling::HandleError::new(
        service,
        |err: Box<dyn std::error::Error + Send + Sync>| async move {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("JSON-RPC error: {err}"),
            )
        },
    );
    Ok(Router::new().fallback_service(svc))
}
