// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use anyhow::anyhow;
use async_trait::async_trait;
use ed25519_dalek::ed25519::signature::Signature;
use jsonrpsee::core::RpcResult;
use sui_core::gateway_types::{TransactionEffectsResponse, TransactionResponse};
use tracing::debug;

use sui_core::gateway_state::{GatewayClient, GatewayState, GatewayTxSeqNumber};
use sui_core::gateway_types::GetObjectInfoResponse;
use sui_core::sui_json::SuiJsonValue;
use sui_types::sui_serde::Base64;
use sui_types::{
    base_types::{ObjectID, SuiAddress, TransactionDigest},
    crypto,
    crypto::SignableBytes,
    messages::{Transaction, TransactionData},
};

use crate::rpc_gateway::responses::SuiTypeTag;
use crate::{
    api::{RpcGatewayServer, TransactionBytes},
    config::{GatewayConfig, PersistedConfig},
    rpc_gateway::responses::ObjectResponse,
};

pub mod responses;

pub struct RpcGatewayImpl {
    gateway: GatewayClient,
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
        Ok(Self { gateway })
    }
}

#[async_trait]
impl RpcGatewayServer for RpcGatewayImpl {
    async fn transfer_coin(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .gateway
            .transfer_coin(signer, object_id, gas, gas_budget, recipient)
            .await?;
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn publish(
        &self,
        sender: SuiAddress,
        compiled_modules: Vec<Base64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let compiled_modules = compiled_modules
            .into_iter()
            .map(|data| data.to_vec())
            .collect::<Result<Vec<_>, _>>()?;
        let data = self
            .gateway
            .publish(sender, compiled_modules, gas, gas_budget)
            .await?;

        Ok(TransactionBytes::from_data(data)?)
    }

    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .gateway
            .split_coin(signer, coin_object_id, split_amounts, gas, gas_budget)
            .await?;
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn merge_coin(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .gateway
            .merge_coins(signer, primary_coin, coin_to_merge, gas, gas_budget)
            .await?;
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn get_owned_objects(&self, owner: SuiAddress) -> RpcResult<ObjectResponse> {
        debug!("get_objects : {}", owner);
        let objects = self.gateway.get_owned_objects(owner).await?;
        Ok(ObjectResponse { objects })
    }

    async fn get_object_info(&self, object_id: ObjectID) -> RpcResult<GetObjectInfoResponse> {
        Ok(self.gateway.get_object_info(object_id).await?)
    }

    async fn execute_transaction(
        &self,
        tx_bytes: Base64,
        signature: Base64,
        pub_key: Base64,
    ) -> RpcResult<TransactionResponse> {
        let data = TransactionData::from_signable_bytes(&tx_bytes.to_vec()?)?;
        let signature =
            crypto::Signature::from_bytes(&[&*signature.to_vec()?, &*pub_key.to_vec()?].concat())
                .map_err(|e| anyhow!(e))?;
        let result = self
            .gateway
            .execute_transaction(Transaction::new(data, signature))
            .await;
        Ok(result?)
    }

    async fn move_call(
        &self,
        signer: SuiAddress,
        package_object_id: ObjectID,
        module: String,
        function: String,
        type_arguments: Vec<SuiTypeTag>,
        rpc_arguments: Vec<SuiJsonValue>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let data = async {
            self.gateway
                .move_call(
                    signer,
                    package_object_id,
                    module,
                    function,
                    type_arguments
                        .into_iter()
                        .map(|tag| tag.try_into())
                        .collect::<Result<Vec<_>, _>>()?,
                    rpc_arguments,
                    gas,
                    gas_budget,
                )
                .await
        }
        .await?;
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn sync_account_state(&self, address: SuiAddress) -> RpcResult<()> {
        debug!("sync_account_state : {}", address);
        self.gateway.sync_account_state(address).await?;
        Ok(())
    }

    async fn get_total_transaction_number(&self) -> RpcResult<u64> {
        Ok(self.gateway.get_total_transaction_number()?)
    }

    async fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.gateway.get_transactions_in_range(start, end)?)
    }

    async fn get_recent_transactions(
        &self,
        count: u64,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.gateway.get_recent_transactions(count)?)
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<TransactionEffectsResponse> {
        Ok(self.gateway.get_transaction(digest).await?)
    }
}
