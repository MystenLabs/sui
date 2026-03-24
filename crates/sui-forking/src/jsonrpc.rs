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

use std::sync::Arc;

use axum::Router;
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::types::ErrorObjectOwned;
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiObjectResponse, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_types::{
    base_types::ObjectID,
    effects::TransactionEffectsAPI,
    quorum_driver_types::ExecuteTransactionRequestType,
    storage::ReadStore,
    transaction::TransactionData,
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
            .map_err(|e| internal_error(format!("Transaction execution failed: {e}")))?;

        let options = options.unwrap_or_default();
        let mut response = SuiTransactionBlockResponse::default();
        response.digest = *result.effects.transaction_digest();

        if options.show_effects {
            response.effects = Some(result.effects.try_into().map_err(|e: anyhow::Error| {
                internal_error(format!("Failed to convert effects: {e}"))
            })?);
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
        let options = options.unwrap_or_default();
        let sim = self.context.simulacrum.read().await;
        let store = sim.store();

        let object = store
            .get_object(&object_id)
            .map_err(|e| internal_error(format!("Failed to read object: {e}")))?;

        match object {
            Some(obj) => {
                let obj_data = sui_json_rpc_types::SuiObjectData::try_from_object_read(
                    sui_types::object::ObjectRead::Exists(
                        obj.compute_object_reference(),
                        obj,
                        None, // layout resolution not needed for BCS
                    ),
                    &options,
                )
                .map_err(|e| internal_error(format!("Failed to convert object: {e}")))?;

                Ok(SuiObjectResponse::new_with_data(obj_data))
            }
            None => Ok(SuiObjectResponse::new_with_error(
                sui_json_rpc_types::SuiObjectResponseError::NotExists { object_id },
            )),
        }
    }

    async fn get_chain_identifier(&self) -> Result<String, ErrorObjectOwned> {
        Ok(self.context.chain_id.to_string())
    }
}

/// Creates an axum Router that serves JSON-RPC on POST `/`.
pub fn create_jsonrpc_router(context: Context) -> anyhow::Result<Router> {
    use jsonrpsee::server::RpcModule;

    let server = ForkingApiImpl { context };
    let mut module = RpcModule::new(());
    module.merge(server.into_rpc()).map_err(|e| {
        anyhow::anyhow!("Failed to merge JSON-RPC module: {e}")
    })?;

    let service = jsonrpsee::server::ServerBuilder::default()
        .build_from_tower(module)?;

    // jsonrpsee tower service can be converted to an axum route
    Ok(Router::new().fallback_service(service))
}
