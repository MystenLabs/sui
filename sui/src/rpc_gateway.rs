// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;
use std::path::Path;
use std::result;
use std::sync::Arc;

use ed25519_dalek::ed25519::signature::Signature;
use jsonrpc_core::{BoxFuture, ErrorCode, Result};
use jsonrpc_derive::rpc;
use jsonrpc_http_server::ServerBuilder;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use serde::Deserialize;
use serde::Serialize;
use serde_with::serde_as;
use tokio::sync::Mutex;

use sui::config::PersistedConfig;
use sui::gateway::GatewayConfig;
use sui::rest_gateway::responses::{NamedObjectRef, ObjectResponse, TransactionBytes};
use sui::sui_config_dir;
use sui_core::gateway_state::gateway_responses::TransactionResponse;
use sui_core::gateway_state::{GatewayClient, GatewayState};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto;
use sui_types::crypto::SignableBytes;
use sui_types::messages::{Transaction, TransactionData};
use sui_types::object::ObjectRead;

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

    #[rpc(name = "object_info")]
    fn object_info(&self, object_id: ObjectID) -> BoxFuture<Result<ObjectRead>>;

    #[rpc(name = "execute_transaction")]
    fn execute_transaction(
        &self,
        tx_bytes: Base64EncodedBytes,
        signature: Base64EncodedBytes,
        pub_key: Base64EncodedBytes,
    ) -> BoxFuture<Result<TransactionResponse>>;

    #[rpc(name = "move_call")]
    fn move_call(
        &self,
        signer: SuiAddress,
        package_object_id: ObjectID,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        pure_arguments: Vec<Base64EncodedBytes>,
        gas_object_id: ObjectID,
        gas_budget: u64,
        object_arguments: Vec<ObjectID>,
        shared_object_arguments: Vec<ObjectID>,
    ) -> BoxFuture<Result<TransactionBytes>>;
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

    fn object_info(&self, object_id: ObjectID) -> BoxFuture<Result<ObjectRead>> {
        let gateway = self.gateway.clone();
        Box::pin(async move {
            let object_read = gateway
                .lock()
                .await
                .get_object_info(object_id)
                .await
                .map_err(to_internal_error)?;
            Ok(object_read)
        })
    }

    fn execute_transaction(
        &self,
        tx_bytes: Base64EncodedBytes,
        signature: Base64EncodedBytes,
        pub_key: Base64EncodedBytes,
    ) -> BoxFuture<Result<TransactionResponse>> {
        let gateway = self.gateway.clone();
        Box::pin(async move {
            async {
                let data = TransactionData::from_signable_bytes(&tx_bytes)?;
                let signature = crypto::Signature::from_bytes(&[&*signature, &*pub_key].concat())?;
                gateway
                    .lock()
                    .await
                    .execute_transaction(Transaction::new(data, signature))
                    .await
            }
            .await
            .map_err(to_internal_error)
        })
    }

    fn move_call(
        &self,
        signer: SuiAddress,
        package_object_id: ObjectID,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        pure_arguments: Vec<Base64EncodedBytes>,
        gas_object_id: ObjectID,
        gas_budget: u64,
        object_arguments: Vec<ObjectID>,
        shared_object_arguments: Vec<ObjectID>,
    ) -> BoxFuture<Result<TransactionBytes>> {
        let gateway = self.gateway.clone();

        Box::pin(async move {
            let mut gateway = gateway.lock().await;

            let data = async {
                let package_object_ref = gateway
                    .get_object_info(package_object_id)
                    .await?
                    .reference()?;
                // Fetch the object info for the gas obj
                let gas_obj_ref = gateway.get_object_info(gas_object_id).await?.reference()?;

                // Fetch the objects for the object args
                let mut object_args_refs = Vec::new();
                for obj_id in object_arguments {
                    let object_ref = gateway.get_object_info(obj_id).await?.reference()?;
                    object_args_refs.push(object_ref);
                }
                let pure_arguments = pure_arguments
                    .iter()
                    .map(|arg| arg.to_vec())
                    .collect::<Vec<_>>();

                gateway
                    .move_call(
                        signer,
                        package_object_ref,
                        module,
                        function,
                        type_arguments,
                        gas_obj_ref,
                        object_args_refs,
                        shared_object_arguments,
                        pure_arguments,
                        gas_budget,
                    )
                    .await
            }
            .await
            .map_err(to_internal_error)?;

            Ok(TransactionBytes::new(data))
        })
    }
}

fn to_internal_error(e: anyhow::Error) -> jsonrpc_core::Error {
    jsonrpc_core::Error {
        code: ErrorCode::InternalError,
        message: e.to_string(),
        data: None,
    }
}

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct Base64EncodedBytes(#[serde_as(as = "serde_with::base64::Base64")] Vec<u8>);

impl Deref for Base64EncodedBytes {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
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
