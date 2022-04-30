// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::rpc_gateway::responses::ObjectResponse;
use crate::rpc_gateway::{
    RpcCallArg, RpcGatewayClient as RpcGateway, SignedTransaction, TransactionBytes,
};
use anyhow::Error;
use async_trait::async_trait;
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use sui_core::gateway_state::gateway_responses::{TransactionEffectsResponse, TransactionResponse};
use sui_core::gateway_state::{GatewayAPI, GatewayTxSeqNumber};
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress, TransactionDigest};
use sui_types::json_schema::Base64;
use sui_types::messages::{CallArg, Transaction, TransactionData};
use sui_types::object::ObjectRead;
use tokio::runtime::Handle;

pub struct RpcGatewayClient {
    client: HttpClient,
}

impl RpcGatewayClient {
    pub fn new(server_url: String) -> Result<Self, anyhow::Error> {
        let client = HttpClientBuilder::default().build(&server_url)?;
        Ok(Self { client })
    }
}

#[async_trait]
impl GatewayAPI for RpcGatewayClient {
    async fn execute_transaction(&self, tx: Transaction) -> Result<TransactionResponse, Error> {
        let signature = tx.tx_signature;
        let signed_tx = SignedTransaction {
            tx_bytes: tx.data.to_bytes(),
            signature: signature.signature_bytes().to_vec(),
            pub_key: signature.public_key_bytes().to_vec(),
        };

        Ok(self.client.execute_transaction(signed_tx).await?)
    }

    async fn transfer_coin(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> Result<TransactionData, Error> {
        let bytes: TransactionBytes = self
            .client
            .transfer_coin(signer, object_id, gas_payment, gas_budget, recipient)
            .await?;
        bytes.to_data()
    }

    async fn sync_account_state(&self, account_addr: SuiAddress) -> Result<(), Error> {
        self.client.sync_account_state(account_addr).await?;
        Ok(())
    }

    async fn move_call(
        &self,
        signer: SuiAddress,
        package_object_ref: ObjectRef,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        gas_object_ref: ObjectRef,
        arguments: Vec<CallArg>,
        gas_budget: u64,
    ) -> Result<TransactionData, Error> {
        let arguments = arguments
            .into_iter()
            .map(|arg| match arg {
                CallArg::Pure(bytes) => RpcCallArg::Pure(Base64(bytes)),
                CallArg::ImmOrOwnedObject((id, _, _)) => RpcCallArg::ImmOrOwnedObject(id),
                CallArg::SharedObject(id) => RpcCallArg::SharedObject(id),
            })
            .collect();

        let bytes: TransactionBytes = self
            .client
            .move_call(
                signer,
                package_object_ref.0,
                module,
                function,
                Some(type_arguments),
                arguments,
                gas_object_ref.0,
                gas_budget,
            )
            .await?;
        bytes.to_data()
    }

    async fn publish(
        &self,
        signer: SuiAddress,
        package_bytes: Vec<Vec<u8>>,
        gas_object_ref: ObjectRef,
        gas_budget: u64,
    ) -> Result<TransactionData, Error> {
        let package_bytes = package_bytes.into_iter().map(Base64).collect();
        let bytes: TransactionBytes = self
            .client
            .publish(signer, package_bytes, gas_object_ref.0, gas_budget)
            .await?;
        bytes.to_data()
    }

    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas_payment: ObjectID,
        gas_budget: u64,
    ) -> Result<TransactionData, Error> {
        let bytes: TransactionBytes = self
            .client
            .split_coin(
                signer,
                coin_object_id,
                split_amounts,
                gas_payment,
                gas_budget,
            )
            .await?;
        bytes.to_data()
    }

    async fn merge_coins(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas_payment: ObjectID,
        gas_budget: u64,
    ) -> Result<TransactionData, Error> {
        let bytes: TransactionBytes = self
            .client
            .merge_coin(signer, primary_coin, coin_to_merge, gas_payment, gas_budget)
            .await?;
        bytes.to_data()
    }

    async fn get_object_info(&self, object_id: ObjectID) -> Result<ObjectRead, Error> {
        Ok(self.client.get_object_info(object_id).await?)
    }

    async fn get_owned_objects(&self, account_addr: SuiAddress) -> Result<Vec<ObjectRef>, Error> {
        let object_response: ObjectResponse = self.client.get_owned_objects(account_addr).await?;
        let object_refs = object_response
            .objects
            .into_iter()
            .map(|o| o.to_object_ref())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(object_refs)
    }

    fn get_total_transaction_number(&self) -> Result<u64, Error> {
        let handle = Handle::current();
        let _ = handle.enter();
        Ok(futures::executor::block_on(
            self.client.get_total_transaction_number(),
        )?)
    }

    fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>, Error> {
        let handle = Handle::current();
        let _ = handle.enter();
        Ok(futures::executor::block_on(
            self.client.get_transactions_in_range(start, end),
        )?)
    }

    fn get_recent_transactions(
        &self,
        count: u64,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>, Error> {
        let handle = Handle::current();
        let _ = handle.enter();
        Ok(futures::executor::block_on(
            self.client.get_recent_transactions(count),
        )?)
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<TransactionEffectsResponse, Error> {
        Ok(self.client.get_transaction(digest).await?)
    }
}
