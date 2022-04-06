// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::net::SocketAddr;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use ed25519_dalek::ed25519::signature::Signature;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_server::{HttpServerBuilder, HttpServerHandle};
use jsonrpsee::RpcModule;
use jsonrpsee_proc_macros::rpc;
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

#[rpc(server, client, namespace = "sui")]
pub trait RPCGateway {
    #[method(name = "create_coin_transfer")]
    async fn create_coin_transfer(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> RpcResult<TransactionBytes>;

    #[method(name = "get_objects")]
    async fn get_objects(&self, owner: SuiAddress) -> RpcResult<ObjectResponse>;

    #[method(name = "get_object_info")]
    async fn get_object_info(&self, object_id: ObjectID) -> RpcResult<ObjectRead>;

    #[method(name = "execute_transaction")]
    async fn execute_transaction(
        &self,
        tx_bytes: Base64EncodedBytes,
        signature: Base64EncodedBytes,
        pub_key: Base64EncodedBytes,
    ) -> RpcResult<TransactionResponse>;

    #[method(name = "create_move_call")]
    async fn create_move_call(
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
    ) -> RpcResult<TransactionBytes>;
}

pub struct RPCGatewayImpl {
    gateway: Arc<Mutex<GatewayClient>>,
}

impl RPCGatewayImpl {
    fn new(config_path: &Path) -> anyhow::Result<Self> {
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

#[async_trait]
impl RPCGatewayServer for RPCGatewayImpl {
    async fn create_coin_transfer(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .gateway
            .lock()
            .await
            .transfer_coin(signer, object_id, gas_payment, gas_budget, recipient)
            .await?;
        Ok(TransactionBytes::new(data))
    }

    async fn get_objects(&self, owner: SuiAddress) -> RpcResult<ObjectResponse> {
        let objects = self
            .gateway
            .lock()
            .await
            .get_owned_objects(owner)?
            .into_iter()
            .map(NamedObjectRef::from)
            .collect();
        Ok(ObjectResponse { objects })
    }

    async fn get_object_info(&self, object_id: ObjectID) -> RpcResult<ObjectRead> {
        Ok(self.gateway.lock().await.get_object_info(object_id).await?)
    }

    async fn execute_transaction(
        &self,
        tx_bytes: Base64EncodedBytes,
        signature: Base64EncodedBytes,
        pub_key: Base64EncodedBytes,
    ) -> RpcResult<TransactionResponse> {
        let data = TransactionData::from_signable_bytes(&tx_bytes)?;
        let signature = crypto::Signature::from_bytes(&[&*signature, &*pub_key].concat())
            .map_err(|e| anyhow!(e))?;
        Ok(self
            .gateway
            .lock()
            .await
            .execute_transaction(Transaction::new(data, signature))
            .await?)
    }

    async fn create_move_call(
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
    ) -> RpcResult<TransactionBytes> {
        let data = async {
            let mut gateway = self.gateway.lock().await;
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
        .await?;
        Ok(TransactionBytes::new(data))
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .expect("setting default subscriber failed");

    let (server_addr, handle) = run_server().await?;
    println!("http://{}", server_addr);

    handle.await;
    Ok(())
}

async fn run_server() -> anyhow::Result<(SocketAddr, HttpServerHandle)> {
    let config_path = sui_config_dir()?.join("gateway.conf");

    let server = HttpServerBuilder::default()
        .build("127.0.0.1:0".parse::<SocketAddr>()?)
        .await?;

    let mut module = RpcModule::new(());
    module.merge(RPCGatewayImpl::new(&config_path)?.into_rpc())?;

    let addr = server.local_addr()?;
    let server_handle = server.start(module)?;
    Ok((addr, server_handle))
}
