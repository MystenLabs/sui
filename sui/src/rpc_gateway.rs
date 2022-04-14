// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use ed25519_dalek::ed25519::signature::Signature;
use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use serde::Deserialize;
use serde::Serialize;
use serde_with::serde_as;
use tokio::sync::Mutex;
use tracing::debug;

use serde_with::base64::Base64;
use sui_core::gateway_state::gateway_responses::TransactionResponse;
use sui_core::gateway_state::{GatewayClient, GatewayState, GatewayTxSeqNumber};
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::crypto;
use sui_types::crypto::SignableBytes;
use sui_types::messages::{Transaction, TransactionData};
use sui_types::object::ObjectRead;

use crate::config::PersistedConfig;
use crate::gateway::GatewayConfig;
use crate::rest_gateway::responses::{NamedObjectRef, ObjectResponse};

#[rpc(server, client, namespace = "gateway")]
pub trait RpcGateway {
    #[method(name = "get_object_info")]
    async fn get_object_info(&self, object_id: ObjectID) -> RpcResult<ObjectRead>;

    #[method(name = "transfer_coin")]
    async fn transfer_coin(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> RpcResult<TransactionBytes>;

    #[method(name = "move_call")]
    async fn move_call(
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

    #[method(name = "publish")]
    async fn publish(
        &self,
        sender: SuiAddress,
        compiled_modules: Vec<Base64EncodedBytes>,
        gas_object_id: ObjectID,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    #[method(name = "split_coin")]
    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas_payment: ObjectID,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    #[method(name = "merge_coins")]
    async fn merge_coin(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    #[method(name = "execute_transaction")]
    async fn execute_transaction(
        &self,
        signed_transaction: SignedTransaction,
    ) -> RpcResult<TransactionResponse>;

    #[method(name = "sync_account_state")]
    async fn sync_account_state(&self, address: SuiAddress) -> RpcResult<()>;

    #[method(name = "get_owned_objects")]
    async fn get_owned_objects(&self, owner: SuiAddress) -> RpcResult<ObjectResponse>;

    #[method(name = "get_total_transaction_number")]
    async fn get_total_transaction_number(&self) -> RpcResult<u64>;

    #[method(name = "get_transactions_in_range")]
    async fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    #[method(name = "get_recent_transactions")]
    async fn get_recent_transactions(
        &self,
        count: u64,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;
}

pub struct RpcGatewayImpl {
    gateway: Arc<Mutex<GatewayClient>>,
}

impl RpcGatewayImpl {
    pub fn new(config_path: &Path) -> anyhow::Result<Self> {
        let config: GatewayConfig = PersistedConfig::read(config_path).map_err(|e| {
            anyhow!(
                "Failed to read config file at {:?}: {}. Have you run `sui genesis` first?",
                config_path,
                e
            )
        })?;
        let committee = config.make_committee();
        let authority_clients = config.make_authority_clients();
        let gateway = Box::new(GatewayState::new(
            config.db_folder_path,
            committee,
            authority_clients,
        )?);
        Ok(Self {
            gateway: Arc::new(Mutex::new(gateway)),
        })
    }
}

#[async_trait]
impl RpcGatewayServer for RpcGatewayImpl {
    async fn transfer_coin(
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
        Ok(TransactionBytes {
            tx_bytes: data.to_bytes(),
        })
    }

    async fn publish(
        &self,
        sender: SuiAddress,
        compiled_modules: Vec<Base64EncodedBytes>,
        gas_object_id: ObjectID,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let compiled_modules = compiled_modules
            .into_iter()
            .map(|data| data.to_vec())
            .collect::<Vec<_>>();

        let mut gateway = self.gateway.lock().await;
        let gas_obj_ref = gateway
            .get_object_info(gas_object_id)
            .await?
            .reference()
            .map_err(|e| anyhow!(e))?;

        let data = gateway
            .publish(sender, compiled_modules, gas_obj_ref, gas_budget)
            .await?;

        Ok(TransactionBytes {
            tx_bytes: data.to_bytes(),
        })
    }

    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas_payment: ObjectID,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .gateway
            .lock()
            .await
            .split_coin(
                signer,
                coin_object_id,
                split_amounts,
                gas_payment,
                gas_budget,
            )
            .await?;
        Ok(TransactionBytes {
            tx_bytes: data.to_bytes(),
        })
    }

    async fn merge_coin(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .gateway
            .lock()
            .await
            .merge_coins(signer, primary_coin, coin_to_merge, gas_payment, gas_budget)
            .await?;
        Ok(TransactionBytes {
            tx_bytes: data.to_bytes(),
        })
    }

    async fn get_owned_objects(&self, owner: SuiAddress) -> RpcResult<ObjectResponse> {
        debug!("get_objects : {}", owner);
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
        signed_tx: SignedTransaction,
    ) -> RpcResult<TransactionResponse> {
        let data = TransactionData::from_signable_bytes(&signed_tx.tx_bytes)?;
        let signature =
            crypto::Signature::from_bytes(&[&*signed_tx.signature, &*signed_tx.pub_key].concat())
                .map_err(|e| anyhow!(e))?;
        Ok(self
            .gateway
            .lock()
            .await
            .execute_transaction(Transaction::new(data, signature))
            .await?)
    }

    async fn move_call(
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
        Ok(TransactionBytes {
            tx_bytes: data.to_bytes(),
        })
    }

    async fn sync_account_state(&self, address: SuiAddress) -> RpcResult<()> {
        debug!("sync_account_state : {}", address);
        self.gateway
            .lock()
            .await
            .sync_account_state(address)
            .await?;
        Ok(())
    }

    async fn get_total_transaction_number(&self) -> RpcResult<u64> {
        Ok(self.gateway.lock().await.get_total_transaction_number()?)
    }

    async fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self
            .gateway
            .lock()
            .await
            .get_transactions_in_range(start, end)?)
    }

    async fn get_recent_transactions(
        &self,
        count: u64,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.gateway.lock().await.get_recent_transactions(count)?)
    }
}

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct SignedTransaction {
    #[serde_as(as = "Base64")]
    pub tx_bytes: Vec<u8>,
    #[serde_as(as = "Base64")]
    pub signature: Vec<u8>,
    #[serde_as(as = "Base64")]
    pub pub_key: Vec<u8>,
}

impl SignedTransaction {
    pub fn new(tx_bytes: Vec<u8>, signature: crypto::Signature) -> Self {
        let signature_bytes = signature.as_bytes();
        Self {
            tx_bytes,
            signature: signature_bytes[..32].to_vec(),
            pub_key: signature_bytes[32..].to_vec(),
        }
    }
}

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct TransactionBytes {
    #[serde_as(as = "Base64")]
    pub tx_bytes: Vec<u8>,
}

impl TransactionBytes {
    pub fn to_data(self) -> Result<TransactionData, anyhow::Error> {
        TransactionData::from_signable_bytes(&self.tx_bytes)
    }
}

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct Base64EncodedBytes(#[serde_as(as = "serde_with::base64::Base64")] pub Vec<u8>);

impl Deref for Base64EncodedBytes {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
