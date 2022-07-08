// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO: Refactor this and sui-json-rpc::api
// This file is a copy of sui-json-rpc::api, ideally we only need 1 copy of api to share between client and server
// However the jsonrpsee rpc proc marco removes the Api traits when generate the RpcServer and RpcClient implementation,
// we might want to replace the proc marco with handcrafted PRC server methods to make it easier to maintain.
// Note: Please make sure to update sui-json-rpc::api if you are adding methods to the Api
use anyhow::Result;
use async_trait::async_trait;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    GatewayTxSeqNumber, GetObjectDataResponse, GetRawObjectDataResponse,
    RPCTransactionRequestParams, SuiEventEnvelope, SuiEventFilter, SuiObjectInfo, SuiTypeTag,
    TransactionBytes, TransactionEffectsResponse, TransactionResponse,
};
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::sui_serde::Base64;
#[async_trait]
pub trait RpcGatewayApi {
    /// Execute the transaction using the transaction data, signature and public key.
    async fn execute_transaction(
        &self,
        tx_bytes: Base64,
        signature: Base64,
        pub_key: Base64,
    ) -> Result<TransactionResponse>;
}

#[async_trait]
pub trait WalletSyncApi {
    /// Synchronize client state with validators.
    async fn sync_account_state(&self, address: SuiAddress) -> Result<()>;
}

#[async_trait]
pub trait RpcReadApi {
    /// Return the list of objects owned by an address.
    async fn get_objects_owned_by_address(&self, address: SuiAddress)
        -> Result<Vec<SuiObjectInfo>>;

    async fn get_objects_owned_by_object(&self, object_id: ObjectID) -> Result<Vec<SuiObjectInfo>>;

    async fn get_total_transaction_number(&self) -> Result<u64>;

    async fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    async fn get_recent_transactions(
        &self,
        count: u64,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<TransactionEffectsResponse>;

    /// Return the object information for a specified object
    async fn get_object(&self, object_id: ObjectID) -> Result<GetObjectDataResponse>;
}

#[async_trait]
pub trait RpcFullNodeReadApi {
    async fn get_transactions_by_input_object(
        &self,
        object: ObjectID,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    async fn get_transactions_by_mutated_object(
        &self,
        object: ObjectID,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    async fn get_transactions_by_move_function(
        &self,
        package: ObjectID,
        module: Option<String>,
        function: Option<String>,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    async fn get_transactions_from_addr(
        &self,
        addr: SuiAddress,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    async fn get_transactions_to_addr(
        &self,
        addr: SuiAddress,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;
}

#[async_trait]
pub trait RpcTransactionBuilder {
    /// Create a transaction to transfer an object from one address to another. The object's type
    /// must allow public transfers
    async fn transfer_object(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> Result<TransactionBytes>;

    /// Send SUI coin object to a Sui address. The SUI object is also used as the gas object.
    async fn transfer_sui(
        &self,
        signer: SuiAddress,
        sui_object_id: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
        amount: Option<u64>,
    ) -> Result<TransactionBytes>;

    /// Execute a Move call transaction by calling the specified function in the module of a given package.
    async fn move_call(
        &self,
        signer: SuiAddress,
        package_object_id: ObjectID,
        module: String,
        function: String,
        type_arguments: Vec<SuiTypeTag>,
        arguments: Vec<SuiJsonValue>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionBytes>;

    /// Publish Move module.
    async fn publish(
        &self,
        sender: SuiAddress,
        compiled_modules: Vec<Base64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionBytes>;

    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionBytes>;

    async fn merge_coin(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionBytes>;

    async fn batch_transaction(
        &self,
        signer: SuiAddress,
        single_transaction_params: Vec<RPCTransactionRequestParams>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> Result<TransactionBytes>;
}

#[async_trait]
pub trait RpcBcsApi {
    /// Return the raw BCS serialised move object bytes for a specified object
    async fn get_raw_object(&self, object_id: ObjectID) -> Result<GetRawObjectDataResponse>;
}

pub trait EventStreamingApi {
    fn subscribe_event(&self, filter: SuiEventFilter);
}

#[async_trait]
pub trait EventReadApi {
    async fn get_events_by_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<Vec<SuiEventEnvelope>>;

    async fn get_events_by_module(
        &self,
        package: ObjectID,
        module: String,
        count: u64,
        start_time: u64,
        end_time: u64,
    ) -> Result<Vec<SuiEventEnvelope>>;

    async fn get_events_by_event_type(
        &self,
        event_type: String,
        count: u64,
        start_time: u64,
        end_time: u64,
    ) -> Result<Vec<SuiEventEnvelope>>;

    async fn get_events_by_sender(
        &self,
        sender: SuiAddress,
        count: u64,
        start_time: u64,
        end_time: u64,
    ) -> Result<Vec<SuiEventEnvelope>>;

    async fn get_events_by_object(
        &self,
        object: ObjectID,
        count: u64,
        start_time: u64,
        end_time: u64,
    ) -> Result<Vec<SuiEventEnvelope>>;

    async fn get_events_by_owner(
        &self,
        owner: SuiAddress,
        count: u64,
        start_time: u64,
        end_time: u64,
    ) -> Result<Vec<SuiEventEnvelope>>;
}
