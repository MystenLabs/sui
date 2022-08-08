// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use anyhow::anyhow;
use futures::StreamExt;
use futures_core::Stream;
use jsonrpsee::core::client::Subscription;
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use rpc_types::SuiExecuteTransactionResponse;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::RwLock;

// re-export essential sui crates
pub use sui_config::gateway;
use sui_config::gateway::GatewayConfig;
use sui_core::gateway_state::{GatewayClient, GatewayState};
pub use sui_json as json;
use sui_json::SuiJsonValue;
use sui_json_rpc::api::EventStreamingApiClient;
use sui_json_rpc::api::QuorumDriverApiClient;
use sui_json_rpc::api::RpcBcsApiClient;
use sui_json_rpc::api::RpcFullNodeReadApiClient;
use sui_json_rpc::api::RpcGatewayApiClient;
use sui_json_rpc::api::RpcReadApiClient;
use sui_json_rpc::api::RpcTransactionBuilderClient;
use sui_json_rpc::api::WalletSyncApiClient;
pub use sui_json_rpc_types as rpc_types;
use sui_json_rpc_types::{
    GatewayTxSeqNumber, GetObjectDataResponse, GetRawObjectDataResponse,
    RPCTransactionRequestParams, SuiEventEnvelope, SuiEventFilter, SuiObjectInfo, SuiTypeTag,
    TransactionEffectsResponse, TransactionResponse,
};
pub use sui_types as types;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress, TransactionDigest};
use sui_types::crypto::SignableBytes;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{Transaction, TransactionData};
use sui_types::sui_serde::Base64;
use types::messages::ExecuteTransactionRequestType;

pub mod crypto;

pub struct SuiClient {
    api: Arc<SuiClientApi>,
    transaction_builder: TransactionBuilder,
    read_api: Arc<ReadApi>,
    full_node_api: FullNodeApi,
    event_api: EventApi,
}

struct ClientState {
    api: Arc<SuiClientApi>,
    objects: BTreeMap<ObjectID, GetRawObjectDataResponse>,
    account_owned_objects: BTreeMap<SuiAddress, Vec<SuiObjectInfo>>,
    object_owned_objects: BTreeMap<ObjectID, Vec<SuiObjectInfo>>,
}

impl ClientState {
    async fn get_objects_owned_by_address(
        &mut self,
        address: SuiAddress,
    ) -> anyhow::Result<Vec<SuiObjectInfo>> {
        let result = match self.account_owned_objects.entry(address) {
            Entry::Vacant(entry) => {
                let objects = match &*self.api {
                    SuiClientApi::Rpc(c, _) => c.get_objects_owned_by_address(address).await?,
                    SuiClientApi::Embedded(c) => c.get_objects_owned_by_address(address).await?,
                };
                entry.insert(objects).to_vec()
            }
            Entry::Occupied(entry) => entry.get().to_vec(),
        };
        Ok(result)
    }

    async fn get_objects_owned_by_object(
        &mut self,
        object_id: ObjectID,
    ) -> anyhow::Result<Vec<SuiObjectInfo>> {
        let result = match self.object_owned_objects.entry(object_id) {
            Entry::Vacant(entry) => {
                let objects = match &*self.api {
                    SuiClientApi::Rpc(c, _) => c.get_objects_owned_by_object(object_id).await?,
                    SuiClientApi::Embedded(c) => c.get_objects_owned_by_object(object_id).await?,
                };
                entry.insert(objects).to_vec()
            }
            Entry::Occupied(entry) => entry.get().to_vec(),
        };
        Ok(result)
    }

    async fn get_object(
        &mut self,
        object_id: ObjectID,
    ) -> anyhow::Result<GetRawObjectDataResponse> {
        let result = match self.objects.entry(object_id) {
            Entry::Vacant(entry) => {
                let object = match &*self.api {
                    SuiClientApi::Rpc(c, _) => c.get_raw_object(object_id).await?,
                    SuiClientApi::Embedded(c) => c.get_raw_object(object_id).await?,
                };
                entry.insert(object).clone()
            }
            Entry::Occupied(entry) => entry.get().clone(),
        };
        Ok(result)
    }
}

#[allow(clippy::large_enum_variant)]
enum SuiClientApi {
    Rpc(HttpClient, Option<WsClient>),
    Embedded(GatewayClient),
}

impl SuiClient {
    pub async fn new_rpc_client(
        http_url: &str,
        ws_url: Option<&str>,
    ) -> Result<SuiClient, anyhow::Error> {
        let client = HttpClientBuilder::default().build(http_url)?;

        let ws_client = if let Some(url) = ws_url {
            Some(WsClientBuilder::default().build(url).await?)
        } else {
            None
        };
        Ok(SuiClient::new(SuiClientApi::Rpc(client, ws_client)))
    }

    pub fn new_embedded_client(config: &GatewayConfig) -> Result<SuiClient, anyhow::Error> {
        let state = GatewayState::create_client(config, None)?;
        Ok(SuiClient::new(SuiClientApi::Embedded(state)))
    }
    fn new(api: SuiClientApi) -> Self {
        let api = Arc::new(api);
        let state = Arc::new(RwLock::new(ClientState {
            api: api.clone(),
            objects: Default::default(),
            account_owned_objects: Default::default(),
            object_owned_objects: Default::default(),
        }));
        let read_api = Arc::new(ReadApi {
            api: api.clone(),
            state,
        });
        let full_node_api = FullNodeApi(api.clone());
        let event_api = EventApi(api.clone());
        let transaction_builder = TransactionBuilder {
            api: api.clone(),
            read_api: read_api.clone(),
        };
        SuiClient {
            api,
            transaction_builder,
            read_api,
            full_node_api,
            event_api,
        }
    }
}

pub struct TransactionBuilder {
    api: Arc<SuiClientApi>,
    read_api: Arc<ReadApi>,
}

impl TransactionBuilder {
    async fn select_gas(
        &self,
        signer: SuiAddress,
        budget: u64,
    ) -> Result<ObjectRef, anyhow::Error> {
        let objs = self.read_api.get_objects_owned_by_address(signer).await?;
        let gas_objs = objs
            .iter()
            .filter(|obj| obj.type_ == GasCoin::type_().to_string());

        for obj in gas_objs {
            let response = self.read_api.get_raw_object(obj.object_id).await?;
            let obj = response.object()?;
            let gas: GasCoin = bcs::from_bytes(&obj.data.try_as_move().unwrap().bcs_bytes)?;
            if gas.value() >= budget {
                return Ok(obj.reference.to_object_ref());
            }
        }
        return Err(anyhow!("Cannot find gas coin for signer address [{signer}] with amount sufficient for the budget [{budget}]."));
    }

    pub async fn transfer_object(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> anyhow::Result<TransactionData> {
        let object = self.read_api.get_object_ref(object_id).await?;
        let gas = if let Some(gas) = gas {
            self.read_api.get_object_ref(gas).await?
        } else {
            self.select_gas(signer, gas_budget).await?
        };
        Ok(TransactionData::new_transfer(
            recipient, object, signer, gas, gas_budget,
        ))
    }

    pub async fn transfer_sui(
        &self,
        signer: SuiAddress,
        sui_object_id: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
        amount: Option<u64>,
    ) -> anyhow::Result<TransactionData> {
        let object = self.read_api.get_object_ref(sui_object_id).await?;
        Ok(TransactionData::new_transfer_sui(
            recipient, signer, amount, object, gas_budget,
        ))
    }

    pub async fn move_call(
        &self,
        signer: SuiAddress,
        package_object_id: ObjectID,
        module: String,
        function: String,
        type_arguments: Vec<SuiTypeTag>,
        arguments: Vec<SuiJsonValue>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => {
                let transaction_bytes = c
                    .move_call(
                        signer,
                        package_object_id,
                        module,
                        function,
                        type_arguments,
                        arguments,
                        gas,
                        gas_budget,
                    )
                    .await?;
                TransactionData::from_signable_bytes(&transaction_bytes.tx_bytes.to_vec()?)?
            }
            SuiClientApi::Embedded(c) => {
                c.move_call(
                    signer,
                    package_object_id,
                    module,
                    function,
                    type_arguments,
                    arguments,
                    gas,
                    gas_budget,
                )
                .await?
            }
        })
    }

    pub async fn publish(
        &self,
        sender: SuiAddress,
        compiled_modules: Vec<Vec<u8>>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => {
                let compiled_modules = compiled_modules
                    .iter()
                    .map(|b| Base64::from_bytes(b))
                    .collect();
                let transaction_bytes =
                    c.publish(sender, compiled_modules, gas, gas_budget).await?;
                TransactionData::from_signable_bytes(&transaction_bytes.tx_bytes.to_vec()?)?
            }
            SuiClientApi::Embedded(c) => {
                c.publish(sender, compiled_modules, gas, gas_budget).await?
            }
        })
    }

    pub async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => {
                let transaction_bytes = c
                    .split_coin(signer, coin_object_id, split_amounts, gas, gas_budget)
                    .await?;
                TransactionData::from_signable_bytes(&transaction_bytes.tx_bytes.to_vec()?)?
            }
            SuiClientApi::Embedded(c) => {
                c.split_coin(signer, coin_object_id, split_amounts, gas, gas_budget)
                    .await?
            }
        })
    }

    pub async fn merge_coins(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => {
                let transaction_bytes = c
                    .merge_coin(signer, primary_coin, coin_to_merge, gas, gas_budget)
                    .await?;
                TransactionData::from_signable_bytes(&transaction_bytes.tx_bytes.to_vec()?)?
            }
            SuiClientApi::Embedded(c) => {
                c.merge_coins(signer, primary_coin, coin_to_merge, gas, gas_budget)
                    .await?
            }
        })
    }

    pub async fn batch_transaction(
        &self,
        signer: SuiAddress,
        single_transaction_params: Vec<RPCTransactionRequestParams>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => {
                let transaction_bytes = c
                    .batch_transaction(signer, single_transaction_params, gas, gas_budget)
                    .await?;
                TransactionData::from_signable_bytes(&transaction_bytes.tx_bytes.to_vec()?)?
            }
            SuiClientApi::Embedded(c) => {
                c.batch_transaction(signer, single_transaction_params, gas, gas_budget)
                    .await?
            }
        })
    }
}

pub struct ReadApi {
    api: Arc<SuiClientApi>,
    state: Arc<RwLock<ClientState>>,
}

impl ReadApi {
    pub async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> anyhow::Result<Vec<SuiObjectInfo>> {
        self.state
            .write()
            .await
            .get_objects_owned_by_address(address)
            .await
    }

    pub async fn get_objects_owned_by_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<Vec<SuiObjectInfo>> {
        self.state
            .write()
            .await
            .get_objects_owned_by_object(object_id)
            .await
    }

    pub async fn get_object(&self, object_id: ObjectID) -> anyhow::Result<GetObjectDataResponse> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => c.get_object(object_id).await?,
            SuiClientApi::Embedded(c) => c.get_object(object_id).await?,
        })
    }

    async fn get_object_ref(&self, object_id: ObjectID) -> anyhow::Result<ObjectRef> {
        Ok(self
            .get_object(object_id)
            .await?
            .object()?
            .reference
            .to_object_ref())
    }

    pub async fn get_raw_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<GetRawObjectDataResponse> {
        self.state.write().await.get_object(object_id).await
    }

    pub async fn get_total_transaction_number(&self) -> anyhow::Result<u64> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => c.get_total_transaction_number().await?,
            SuiClientApi::Embedded(c) => c.get_total_transaction_number()?,
        })
    }

    pub async fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> anyhow::Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => c.get_transactions_in_range(start, end).await?,
            SuiClientApi::Embedded(c) => c.get_transactions_in_range(start, end)?,
        })
    }

    pub async fn get_recent_transactions(
        &self,
        count: u64,
    ) -> anyhow::Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => c.get_recent_transactions(count).await?,
            SuiClientApi::Embedded(c) => c.get_recent_transactions(count)?,
        })
    }

    pub async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> anyhow::Result<TransactionEffectsResponse> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => c.get_transaction(digest).await?,
            SuiClientApi::Embedded(c) => c.get_transaction(digest).await?,
        })
    }
}

pub struct FullNodeApi(Arc<SuiClientApi>);

impl FullNodeApi {
    pub async fn get_transactions_by_input_object(
        &self,
        object: ObjectID,
    ) -> anyhow::Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(match &*self.0 {
            SuiClientApi::Rpc(c, _) => c.get_transactions_by_input_object(object).await?,
            SuiClientApi::Embedded(_) => {
                return Err(anyhow!("Method not supported by embedded gateway client."))
            }
        })
    }

    pub async fn get_transactions_by_mutated_object(
        &self,
        object: ObjectID,
    ) -> anyhow::Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(match &*self.0 {
            SuiClientApi::Rpc(c, _) => c.get_transactions_by_mutated_object(object),
            SuiClientApi::Embedded(_) => {
                return Err(anyhow!("Method not supported by embedded gateway client."))
            }
        }
        .await?)
    }

    pub async fn get_transactions_by_move_function(
        &self,
        package: ObjectID,
        module: Option<String>,
        function: Option<String>,
    ) -> anyhow::Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(match &*self.0 {
            SuiClientApi::Rpc(c, _) => {
                c.get_transactions_by_move_function(package, module, function)
            }
            SuiClientApi::Embedded(_) => {
                return Err(anyhow!("Method not supported by embedded gateway client."))
            }
        }
        .await?)
    }

    pub async fn get_transactions_from_addr(
        &self,
        addr: SuiAddress,
    ) -> anyhow::Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(match &*self.0 {
            SuiClientApi::Rpc(c, _) => c.get_transactions_from_addr(addr),
            SuiClientApi::Embedded(_) => {
                return Err(anyhow!("Method not supported by embedded gateway client."))
            }
        }
        .await?)
    }

    pub async fn get_transactions_to_addr(
        &self,
        addr: SuiAddress,
    ) -> anyhow::Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(match &*self.0 {
            SuiClientApi::Rpc(c, _) => c.get_transactions_to_addr(addr),
            SuiClientApi::Embedded(_) => {
                return Err(anyhow!("Method not supported by embedded gateway client."))
            }
        }
        .await?)
    }
}
pub struct EventApi(Arc<SuiClientApi>);

impl EventApi {
    pub async fn subscribe_event(
        &self,
        filter: SuiEventFilter,
    ) -> anyhow::Result<impl Stream<Item = Result<SuiEventEnvelope, anyhow::Error>>> {
        match &*self.0 {
            SuiClientApi::Rpc(_, Some(c)) => {
                let subscription: Subscription<SuiEventEnvelope> =
                    c.subscribe_event(filter).await?;
                Ok(subscription.map(|item| Ok(item?)))
            }
            _ => Err(anyhow!("Subscription only supported by WebSocket client.")),
        }
    }
}

impl SuiClient {
    pub fn transaction_builder(&self) -> &TransactionBuilder {
        &self.transaction_builder
    }

    pub fn read_api(&self) -> &ReadApi {
        &self.read_api
    }

    pub fn full_node_api(&self) -> &FullNodeApi {
        &self.full_node_api
    }

    pub fn event_api(&self) -> &EventApi {
        &self.event_api
    }

    pub async fn execute_transaction(
        &self,
        tx: Transaction,
    ) -> anyhow::Result<SuiTransactionResponse> {
        Ok(match &self {
            Self::Http(c) => {
                let (tx_bytes, flag, signature, pub_key) = tx.to_network_data_for_execution();
                RpcGatewayApiClient::execute_transaction(c, tx_bytes, flag, signature, pub_key)
                    .await?
            }
            Self::Ws(c) => {
                let (tx_bytes, flag, signature, pub_key) = tx.to_network_data_for_execution();
                RpcGatewayApiClient::execute_transaction(c, tx_bytes, flag, signature, pub_key)
                    .await?
            }
            Self::Embedded(c) => c.execute_transaction(tx).await?,
        })
    }

    pub async fn execute_transaction_by_fullnode(
        &self,
        tx: Transaction,
        request_type: ExecuteTransactionRequestType,
    ) -> anyhow::Result<SuiExecuteTransactionResponse> {
        Ok(match &self {
            Self::Http(c) => {
                let (tx_bytes, flag, signature, pub_key) = tx.to_network_data_for_execution();
                QuorumDriverApiClient::execute_transaction(
                    c,
                    tx_bytes,
                    flag,
                    signature,
                    pub_key,
                    request_type,
                )
                .await?
            }
            Self::Ws(c) => {
                let (tx_bytes, flag, signature, pub_key) = tx.to_network_data_for_execution();
                QuorumDriverApiClient::execute_transaction(
                    c,
                    tx_bytes,
                    flag,
                    signature,
                    pub_key,
                    request_type,
                )
                .await?
            }
            // TODO do we want to support an embedded quorum driver?
            Self::Embedded(_c) => unimplemented!(),
        })
    }

    pub async fn sync_client_state(&self, address: SuiAddress) -> anyhow::Result<()> {
        match &*self.api {
            SuiClientApi::Rpc(c, _) => c.sync_account_state(address).await?,
            SuiClientApi::Embedded(c) => c.sync_account_state(address).await?,
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientType {
    Embedded(GatewayConfig),
    RPC(String, Option<String>),
}

impl Display for ClientType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();

        match self {
            ClientType::Embedded(config) => {
                writeln!(writer, "Client Type : Embedded Gateway")?;
                writeln!(
                    writer,
                    "Gateway state DB folder path : {:?}",
                    config.db_folder_path
                )?;
                let authorities = config
                    .validator_set
                    .iter()
                    .map(|info| info.network_address());
                writeln!(
                    writer,
                    "Authorities : {:?}",
                    authorities.collect::<Vec<_>>()
                )?;
            }
            ClientType::RPC(url, ws_url) => {
                writeln!(writer, "Client Type : JSON-RPC")?;
                writeln!(writer, "HTTP RPC URL : {}", url)?;
                writeln!(writer, "WS RPC URL : {:?}", ws_url)?;
            }
        }
        write!(f, "{}", writer)
    }
}

impl ClientType {
    pub async fn init(&self) -> Result<SuiClient, anyhow::Error> {
        Ok(match self {
            ClientType::Embedded(config) => SuiClient::new_embedded_client(config)?,
            ClientType::RPC(url, ws_url) => {
                SuiClient::new_rpc_client(url, ws_url.as_deref()).await?
            }
        })
    }
}
