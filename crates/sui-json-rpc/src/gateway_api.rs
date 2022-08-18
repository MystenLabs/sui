// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee_core::server::rpc_module::RpcModule;
use signature::Signature;
use sui_types::messages::SenderSignedData;
use tracing::debug;

use crate::api::{
    RpcGatewayApiServer, RpcReadApiServer, RpcTransactionBuilderServer, WalletSyncApiServer,
};
use crate::SuiRpcModule;
use sui_core::gateway_state::{GatewayClient, GatewayTxSeqNumber};
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    GetObjectDataResponse, RPCTransactionRequestParams, SuiObjectInfo, SuiTransactionResponse,
    SuiTypeTag, TransactionBytes,
};
use sui_open_rpc::Module;
use sui_types::crypto::SignatureScheme;
use sui_types::sui_serde::Base64;
use sui_types::{
    base_types::{ObjectID, SuiAddress, TransactionDigest},
    crypto,
    crypto::SignableBytes,
    messages::{Transaction, TransactionData},
};

pub struct RpcGatewayImpl {
    client: GatewayClient,
}

pub struct GatewayWalletSyncApiImpl {
    client: GatewayClient,
}

pub struct GatewayReadApiImpl {
    client: GatewayClient,
}

pub struct TransactionBuilderImpl {
    client: GatewayClient,
}

impl RpcGatewayImpl {
    pub fn new(client: GatewayClient) -> Self {
        Self { client }
    }
}

impl GatewayWalletSyncApiImpl {
    pub fn new(client: GatewayClient) -> Self {
        Self { client }
    }
}

impl GatewayReadApiImpl {
    pub fn new(client: GatewayClient) -> Self {
        Self { client }
    }
}
impl TransactionBuilderImpl {
    pub fn new(client: GatewayClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl RpcGatewayApiServer for RpcGatewayImpl {
    async fn execute_transaction(
        &self,
        tx_bytes: Base64,
        sig_scheme: SignatureScheme,
        signature: Base64,
        pub_key: Base64,
    ) -> RpcResult<SuiTransactionResponse> {
        let data = TransactionData::from_signable_bytes(&tx_bytes.to_vec()?)?;
        let flag = vec![sig_scheme.flag()];
        let tx_signature = crypto::Signature::from_bytes(
            &[&*flag, &*signature.to_vec()?, &pub_key.to_vec()?].concat(),
        )
        .map_err(|e| anyhow!(e))?;
        let result = self
            .client
            .execute_transaction(Transaction::new(SenderSignedData { data, tx_signature }))
            .await;
        Ok(result?)
    }
}

impl SuiRpcModule for RpcGatewayImpl {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::RpcGatewayApiOpenRpc::module_doc()
    }
}

#[async_trait]
impl WalletSyncApiServer for GatewayWalletSyncApiImpl {
    async fn sync_account_state(&self, address: SuiAddress) -> RpcResult<()> {
        debug!("sync_account_state : {}", address);
        self.client.sync_account_state(address).await?;
        Ok(())
    }
}

impl SuiRpcModule for GatewayWalletSyncApiImpl {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::WalletSyncApiOpenRpc::module_doc()
    }
}

#[async_trait]
impl RpcReadApiServer for GatewayReadApiImpl {
    async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> RpcResult<Vec<SuiObjectInfo>> {
        debug!("get_objects_own_by_address : {}", address);
        Ok(self.client.get_objects_owned_by_address(address).await?)
    }

    async fn get_objects_owned_by_object(
        &self,
        object_id: ObjectID,
    ) -> RpcResult<Vec<SuiObjectInfo>> {
        debug!("get_objects_own_by_object : {}", object_id);
        Ok(self.client.get_objects_owned_by_object(object_id).await?)
    }

    async fn get_object(&self, object_id: ObjectID) -> RpcResult<GetObjectDataResponse> {
        Ok(self.client.get_object(object_id).await?)
    }

    async fn get_recent_transactions(
        &self,
        count: u64,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.client.get_recent_transactions(count)?)
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<SuiTransactionResponse> {
        Ok(self.client.get_transaction(digest).await?)
    }

    async fn get_total_transaction_number(&self) -> RpcResult<u64> {
        Ok(self.client.get_total_transaction_number()?)
    }

    async fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.client.get_transactions_in_range(start, end)?)
    }
}

impl SuiRpcModule for GatewayReadApiImpl {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::RpcReadApiOpenRpc::module_doc()
    }
}

#[async_trait]
impl RpcTransactionBuilderServer for TransactionBuilderImpl {
    async fn transfer_object(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .client
            .public_transfer_object(signer, object_id, gas, gas_budget, recipient)
            .await?;
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn transfer_sui(
        &self,
        signer: SuiAddress,
        sui_object_id: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
        amount: Option<u64>,
    ) -> RpcResult<TransactionBytes> {
        let data = self
            .client
            .transfer_sui(signer, sui_object_id, gas_budget, recipient, amount)
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
            .client
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
            .client
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
            .client
            .merge_coins(signer, primary_coin, coin_to_merge, gas, gas_budget)
            .await?;
        Ok(TransactionBytes::from_data(data)?)
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
            self.client
                .move_call(
                    signer,
                    package_object_id,
                    module,
                    function,
                    type_arguments,
                    rpc_arguments,
                    gas,
                    gas_budget,
                )
                .await
        }
        .await?;
        Ok(TransactionBytes::from_data(data)?)
    }

    async fn batch_transaction(
        &self,
        signer: SuiAddress,
        params: Vec<RPCTransactionRequestParams>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes> {
        let data = async {
            self.client
                .batch_transaction(signer, params, gas, gas_budget)
                .await
        }
        .await?;
        Ok(TransactionBytes::from_data(data)?)
    }
}

impl SuiRpcModule for TransactionBuilderImpl {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::RpcTransactionBuilderOpenRpc::module_doc()
    }
}
