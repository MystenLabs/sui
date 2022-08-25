// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Write;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use anyhow::anyhow;
use futures::StreamExt;
use futures_core::Stream;
use jsonrpsee::core::client::Subscription;
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use serde::Deserialize;
use serde::Serialize;

use rpc_types::SuiExecuteTransactionResponse;
pub use sui_config::gateway;
use sui_config::gateway::GatewayConfig;
use sui_core::gateway_state::{GatewayClient, GatewayState};
pub use sui_json as json;
use sui_json_rpc::api::EventStreamingApiClient;
use sui_json_rpc::api::QuorumDriverApiClient;
use sui_json_rpc::api::RpcBcsApiClient;
use sui_json_rpc::api::RpcFullNodeReadApiClient;
use sui_json_rpc::api::RpcGatewayApiClient;
use sui_json_rpc::api::RpcReadApiClient;
use sui_json_rpc::api::WalletSyncApiClient;
pub use sui_json_rpc_types as rpc_types;
use sui_json_rpc_types::{
    GatewayTxSeqNumber, GetObjectDataResponse, GetRawObjectDataResponse, SuiEventEnvelope,
    SuiEventFilter, SuiObjectInfo, SuiTransactionResponse,
};
pub use sui_types as types;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::messages::Transaction;
use types::messages::ExecuteTransactionRequestType;

use crate::transaction_builder::TransactionBuilder;

// re-export essential sui crates
pub mod crypto;
mod transaction_builder;

pub struct SuiClient {
    transaction_builder: TransactionBuilder,
    read_api: Arc<ReadApi>,
    full_node_api: FullNodeApi,
    event_api: EventApi,
    quorum_driver: QuorumDriver,
    wallet_sync_api: WalletSyncApi,
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
        let read_api = Arc::new(ReadApi { api: api.clone() });
        let quorum_driver = QuorumDriver { api: api.clone() };

        let full_node_api = FullNodeApi(api.clone());
        let event_api = EventApi(api.clone());
        let transaction_builder = TransactionBuilder(read_api.clone());
        let wallet_sync_api = WalletSyncApi(api);

        SuiClient {
            transaction_builder,
            read_api,
            full_node_api,
            event_api,
            quorum_driver,
            wallet_sync_api,
        }
    }
}

pub struct ReadApi {
    api: Arc<SuiClientApi>,
}

impl ReadApi {
    pub async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> anyhow::Result<Vec<SuiObjectInfo>> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => c.get_objects_owned_by_address(address).await?,
            SuiClientApi::Embedded(c) => c.get_objects_owned_by_address(address).await?,
        })
    }

    pub async fn get_objects_owned_by_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<Vec<SuiObjectInfo>> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => c.get_objects_owned_by_object(object_id).await?,
            SuiClientApi::Embedded(c) => c.get_objects_owned_by_object(object_id).await?,
        })
    }

    pub async fn get_parsed_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<GetObjectDataResponse> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => c.get_object(object_id).await?,
            SuiClientApi::Embedded(c) => c.get_object(object_id).await?,
        })
    }

    pub async fn get_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<GetRawObjectDataResponse> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => c.get_raw_object(object_id).await?,
            SuiClientApi::Embedded(c) => c.get_raw_object(object_id).await?,
        })
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
    ) -> anyhow::Result<SuiTransactionResponse> {
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
pub struct QuorumDriver {
    api: Arc<SuiClientApi>,
}

impl QuorumDriver {
    pub async fn execute_transaction(
        &self,
        tx: Transaction,
    ) -> anyhow::Result<SuiTransactionResponse> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => {
                let (tx_bytes, flag, signature, pub_key) = tx.to_network_data_for_execution();
                RpcGatewayApiClient::execute_transaction(c, tx_bytes, flag, signature, pub_key)
                    .await?
            }
            SuiClientApi::Embedded(c) => c.execute_transaction(tx).await?,
        })
    }

    pub async fn execute_transaction_by_fullnode(
        &self,
        tx: Transaction,
        request_type: ExecuteTransactionRequestType,
    ) -> anyhow::Result<SuiExecuteTransactionResponse> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c, _) => {
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
            SuiClientApi::Embedded(_c) => unimplemented!(),
        })
    }
}

pub struct WalletSyncApi(Arc<SuiClientApi>);

impl WalletSyncApi {
    pub async fn sync_account_state(&self, address: SuiAddress) -> anyhow::Result<()> {
        match &*self.0 {
            SuiClientApi::Rpc(c, _) => c.sync_account_state(address).await?,
            SuiClientApi::Embedded(c) => c.sync_account_state(address).await?,
        }
        Ok(())
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
    pub fn quorum_driver(&self) -> &QuorumDriver {
        &self.quorum_driver
    }
    pub fn wallet_sync_api(&self) -> &WalletSyncApi {
        &self.wallet_sync_api
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientType {
    Embedded(GatewayConfig),
    RPC(
        String,
        #[serde(default, skip_serializing_if = "Option::is_none")] Option<String>,
    ),
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
