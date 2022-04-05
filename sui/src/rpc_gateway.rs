// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::result;
use std::sync::Arc;

use jsonrpc_core::{BoxFuture, ErrorCode, Result};
use jsonrpc_derive::rpc;
use jsonrpc_http_server::ServerBuilder;
use tokio::sync::Mutex;

use sui::config::PersistedConfig;
use sui::gateway::GatewayConfig;
use sui::rest_gateway::responses::{NamedObjectRef, ObjectResponse, TransactionBytes};
use sui::sui_config_dir;
use sui_core::gateway_state::{GatewayClient, GatewayState};
use sui_types::base_types::{ObjectID, SuiAddress};

#[rpc]
pub trait RPCGateway {
    #[rpc(name = "new_transfer")]
    fn new_transfer(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> BoxFuture<Result<TransactionBytes>>;

    #[rpc(name = "objects")]
    fn objects(&self, owner: SuiAddress) -> BoxFuture<Result<ObjectResponse>>;
}

pub struct RPCGatewayImpl {
    gateway: Arc<Mutex<GatewayClient>>,
}

impl RPCGatewayImpl {
    fn new(config_path: &Path) -> result::Result<Self, anyhow::Error> {
        let config: GatewayConfig = PersistedConfig::read(config_path)?;
        let committee = config.make_committee();
        let authority_clients = config.make_authority_clients();
        let gateway = Box::new(GatewayState::new(
            config.db_folder_path,
            committee,
            authority_clients,
        ));
        Ok(Self {
            gateway: Arc::new(Mutex::new(gateway)),
        })
    }
}

impl RPCGateway for RPCGatewayImpl {
    fn new_transfer(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> BoxFuture<Result<TransactionBytes>> {
        let gateway = self.gateway.clone();
        Box::pin(async move {
            let mut gateway = gateway.lock().await;
            Ok(TransactionBytes::new(
                gateway
                    .transfer_coin(signer, object_id, gas_payment, gas_budget, recipient)
                    .await
                    .map_err(|e| jsonrpc_core::Error {
                        code: ErrorCode::InternalError,
                        message: e.to_string(),
                        data: None,
                    })?,
            ))
        })
    }

    fn objects(&self, owner: SuiAddress) -> BoxFuture<Result<ObjectResponse>> {
        let gateway = self.gateway.clone();
        Box::pin(async move {
            let objects = gateway
                .lock()
                .await
                .get_owned_objects(owner)
                .map_err(|e| jsonrpc_core::Error {
                    code: ErrorCode::InternalError,
                    message: e.to_string(),
                    data: None,
                })?
                .into_iter()
                .map(NamedObjectRef::from)
                .collect();
            Ok(ObjectResponse { objects })
        })
    }
}

fn main() -> result::Result<(), anyhow::Error> {
    let mut io = jsonrpc_core::IoHandler::new();

    let config_path = sui_config_dir()?.join("gateway.conf");
    io.extend_with(RPCGatewayImpl::new(&config_path)?.to_delegate());

    let server = ServerBuilder::new(io)
        .threads(3)
        .start_http(&"127.0.0.1:3030".parse().unwrap())?;

    server.wait();
    Ok(())
}
