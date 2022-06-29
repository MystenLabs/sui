// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::rpc_types::SuiEventEnvelope;
use crate::rpc_types::SuiEventFilter;
use crate::rpc_types::{
    GetObjectDataResponse, GetRawObjectDataResponse, RPCTransactionRequestParams,
    SuiInputObjectKind, SuiObjectInfo, SuiObjectRef, SuiTypeTag, TransactionEffectsResponse,
    TransactionResponse,
};
use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sui_json::SuiJsonValue;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::sui_serde::Base64;
use sui_types::{
    base_types::{ObjectID, SuiAddress, TransactionDigest},
    crypto::SignableBytes,
    messages::TransactionData,
};

pub mod client;
pub mod rpc_types;

type GatewayTxSeqNumber = u64;

#[open_rpc(namespace = "sui", tag = "Quorum Driver API")]
#[rpc(server, client, namespace = "sui")]
pub trait QuorumDriverApi {
    /// Execute the transaction using the transaction data, signature and public key.
    #[method(name = "executeTransaction")]
    async fn execute_transaction(
        &self,
        tx_bytes: Base64,
        signature: Base64,
        pub_key: Base64,
    ) -> RpcResult<TransactionResponse>;
}

#[open_rpc(namespace = "sui", tag = "Wallet Sync API")]
#[rpc(server, client, namespace = "sui")]
pub trait WalletSyncApi {
    /// Synchronize client state with validators.
    #[method(name = "syncAccountState")]
    async fn sync_account_state(&self, address: SuiAddress) -> RpcResult<()>;
}

#[open_rpc(namespace = "sui", tag = "Read API")]
#[rpc(server, client, namespace = "sui")]
pub trait RpcReadApi {
    /// Return the list of objects owned by an address.
    #[method(name = "getObjectsOwnedByAddress")]
    async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> RpcResult<Vec<SuiObjectInfo>>;

    #[method(name = "getObjectsOwnedByObject")]
    async fn get_objects_owned_by_object(
        &self,
        object_id: ObjectID,
    ) -> RpcResult<Vec<SuiObjectInfo>>;

    #[method(name = "getTotalTransactionNumber")]
    async fn get_total_transaction_number(&self) -> RpcResult<u64>;

    #[method(name = "getTransactionsInRange")]
    async fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    #[method(name = "getRecentTransactions")]
    async fn get_recent_transactions(
        &self,
        count: u64,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    #[method(name = "getTransaction")]
    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<TransactionEffectsResponse>;

    /// Return the object information for a specified object
    #[method(name = "getObject")]
    async fn get_object(&self, object_id: ObjectID) -> RpcResult<GetObjectDataResponse>;
}

#[open_rpc(namespace = "sui", tag = "Full Node API")]
#[rpc(server, client, namespace = "sui")]
pub trait RpcFullNodeReadApi {
    #[method(name = "getTransactionsByInputObject")]
    async fn get_transactions_by_input_object(
        &self,
        object: ObjectID,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    #[method(name = "getTransactionsByMutatedObject")]
    async fn get_transactions_by_mutated_object(
        &self,
        object: ObjectID,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    #[method(name = "getTransactionsByMoveFunction")]
    async fn get_transactions_by_move_function(
        &self,
        package: ObjectID,
        module: Option<String>,
        function: Option<String>,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    #[method(name = "getTransactionsFromAddress")]
    async fn get_transactions_from_addr(
        &self,
        addr: SuiAddress,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    #[method(name = "getTransactionsToAddress")]
    async fn get_transactions_to_addr(
        &self,
        addr: SuiAddress,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;
}

#[open_rpc(namespace = "sui", tag = "Transaction Builder API")]
#[rpc(server, client, namespace = "sui")]
pub trait RpcTransactionBuilder {
    /// Create a transaction to transfer an object from one address to another. The object's type
    /// must allow public transfers
    #[method(name = "transferObject")]
    async fn public_transfer_object(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> RpcResult<TransactionBytes>;

    /// Send SUI coin object to a Sui address. The SUI object is also used as the gas object.
    #[method(name = "transferSui")]
    async fn transfer_sui(
        &self,
        signer: SuiAddress,
        sui_object_id: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
        amount: Option<u64>,
    ) -> RpcResult<TransactionBytes>;

    /// Execute a Move call transaction by calling the specified function in the module of a given package.
    #[method(name = "moveCall")]
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
    ) -> RpcResult<TransactionBytes>;

    /// Publish Move module.
    #[method(name = "publish")]
    async fn publish(
        &self,
        sender: SuiAddress,
        compiled_modules: Vec<Base64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    #[method(name = "splitCoin")]
    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    #[method(name = "mergeCoins")]
    async fn merge_coin(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    #[method(name = "batchTransaction")]
    async fn batch_transaction(
        &self,
        signer: SuiAddress,
        single_transaction_params: Vec<RPCTransactionRequestParams>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;
}

#[open_rpc(namespace = "sui", tag = "BCS API")]
#[rpc(server, client, namespace = "sui")]
pub trait RpcBcsApi {
    /// Return the raw BCS serialised move object bytes for a specified object
    #[method(name = "getRawObject")]
    async fn get_raw_object(&self, object_id: ObjectID) -> RpcResult<GetRawObjectDataResponse>;
}

#[serde_as]
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransactionBytes {
    pub tx_bytes: Base64,
    pub gas: SuiObjectRef,
    pub input_objects: Vec<SuiInputObjectKind>,
}

impl TransactionBytes {
    pub fn from_data(data: TransactionData) -> Result<Self, anyhow::Error> {
        Ok(Self {
            tx_bytes: Base64::from_bytes(&data.to_bytes()),
            gas: data.gas().into(),
            input_objects: data
                .input_objects()?
                .into_iter()
                .map(SuiInputObjectKind::from)
                .collect(),
        })
    }

    pub fn to_data(self) -> Result<TransactionData, anyhow::Error> {
        TransactionData::from_signable_bytes(&self.tx_bytes.to_vec()?)
    }
}

#[open_rpc(namespace = "sui", tag = "Event Subscription")]
#[rpc(server, client, namespace = "sui")]
pub trait EventStreamingApi {
    #[subscription(name = "subscribeEvent", item = SuiEventEnvelope)]
    fn subscribe_event(&self, filter: SuiEventFilter);
}

#[open_rpc(namespace = "sui", tag = "Event Read API")]
#[rpc(server, client, namespace = "sui")]
pub trait EventReadApi {
    #[method(name = "getEventsByTransaction")]
    async fn get_events_by_transaction(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    #[method(name = "getEventsByModule")]
    async fn get_events_by_module(
        &self,
        package: ObjectID,
        module: String,
        count: u64,
        start_time: u64,
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    #[method(name = "getEventsByEventType")]
    async fn get_events_by_event_type(
        &self,
        event_type: String,
        count: u64,
        start_time: u64,
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    #[method(name = "getEventsBySender")]
    async fn get_events_by_sender(
        &self,
        sender: SuiAddress,
        count: u64,
        start_time: u64,
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    #[method(name = "getEventsByObject")]
    async fn get_events_by_object(
        &self,
        object: ObjectID,
        count: u64,
        start_time: u64,
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;

    #[method(name = "getEventsByOwner")]
    async fn get_events_by_owner(
        &self,
        owner: SuiAddress,
        count: u64,
        start_time: u64,
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>>;
}
