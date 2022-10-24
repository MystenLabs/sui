// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{Debug, Write};
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail};
use async_trait::async_trait;
use futures::StreamExt;
use futures_core::Stream;
use jsonrpsee::core::client::{ClientT, Subscription};
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use rpc_types::{
    GetPastObjectDataResponse, SuiCertifiedTransaction, SuiExecuteTransactionResponse,
    SuiParsedTransactionResponse, SuiTransactionEffects,
};
pub use sui_config::gateway;
use sui_config::gateway::GatewayConfig;
use sui_core::gateway_state::{GatewayClient, GatewayState, TxSeqNumber};
pub use sui_json as json;
use sui_json_rpc::api::EventStreamingApiClient;
use sui_json_rpc::api::RpcBcsApiClient;
use sui_json_rpc::api::RpcFullNodeReadApiClient;
use sui_json_rpc::api::RpcReadApiClient;
use sui_json_rpc::api::TransactionExecutionApiClient;
pub use sui_json_rpc_types as rpc_types;
use sui_json_rpc_types::{
    GetObjectDataResponse, GetRawObjectDataResponse, SuiEventEnvelope, SuiEventFilter,
    SuiObjectInfo, SuiTransactionResponse, TransactionsPage,
};
use sui_transaction_builder::{DataReader, TransactionBuilder};
pub use sui_types as types;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::messages::Transaction;
use sui_types::query::{Ordering, TransactionQuery};
use types::base_types::SequenceNumber;
use types::committee::EpochId;
use types::error::TRANSACTION_NOT_FOUND_MSG_PREFIX;
use types::messages::{CommitteeInfoResponse, ExecuteTransactionRequestType};

const WAIT_FOR_TX_TIMEOUT_SEC: u64 = 10;

#[derive(Debug)]
pub struct TransactionExecutionResult {
    pub tx_digest: TransactionDigest,
    pub tx_cert: Option<SuiCertifiedTransaction>,
    pub effects: Option<SuiTransactionEffects>,
    pub confirmed_local_execution: bool,
    pub timestamp_ms: Option<u64>,
    pub parsed_data: Option<SuiParsedTransactionResponse>,
}

#[derive(Clone)]
pub struct SuiClient {
    api: Arc<SuiClientApi>,
    transaction_builder: TransactionBuilder,
    read_api: Arc<ReadApi>,
    full_node_api: FullNodeApi,
    event_api: EventApi,
    quorum_driver: QuorumDriver,
    wallet_sync_api: WalletSyncApi,
}

#[allow(clippy::large_enum_variant)]
enum SuiClientApi {
    Rpc(RpcClient),
    Embedded(GatewayClient),
}

impl Debug for SuiClientApi {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SuiClientApi::Rpc(rpc_client) => write!(
                f,
                "RPC client. Http: {:?}, Websocket: {:?}",
                rpc_client.http, rpc_client.ws
            ),
            SuiClientApi::Embedded(_) => write!(f, "Embedded Gateway client."),
        }
    }
}

struct RpcClient {
    http: HttpClient,
    ws: Option<WsClient>,
    info: ServerInfo,
}

struct ServerInfo {
    rpc_methods: Vec<String>,
    subscriptions: Vec<String>,
    version: String,
}

impl RpcClient {
    pub async fn new(http: &str, ws: Option<&str>) -> Result<Self, anyhow::Error> {
        let http = HttpClientBuilder::default().build(http)?;
        let ws = if let Some(url) = ws {
            Some(WsClientBuilder::default().build(url).await?)
        } else {
            None
        };
        let info = Self::get_server_info(&http, &ws).await?;
        Ok(Self { http, ws, info })
    }

    async fn get_server_info(
        http: &HttpClient,
        ws: &Option<WsClient>,
    ) -> Result<ServerInfo, anyhow::Error> {
        let rpc_spec: Value = http
            .request("rpc.discover", None)
            .await
            .map_err(|e| anyhow!("Fail to connect to the RPC server: {e}"))?;
        let version = rpc_spec
            .pointer("/info/version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Fail parsing server version from rpc.discover endpoint."))?;
        let rpc_methods = Self::parse_methods(&rpc_spec)?;

        let subscriptions = if let Some(ws) = ws {
            let rpc_spec: Value = ws
                .request("rpc.discover", None)
                .await
                .map_err(|e| anyhow!("Fail to connect to the Websocket server: {e}"))?;
            Self::parse_methods(&rpc_spec)?
        } else {
            Vec::new()
        };
        Ok(ServerInfo {
            rpc_methods,
            subscriptions,
            version: version.to_string(),
        })
    }

    fn parse_methods(server_spec: &Value) -> Result<Vec<String>, anyhow::Error> {
        let methods = server_spec
            .pointer("/methods")
            .and_then(|methods| methods.as_array())
            .ok_or_else(|| {
                anyhow!("Fail parsing server information from rpc.discover endpoint.")
            })?;

        Ok(methods
            .iter()
            .flat_map(|method| method["name"].as_str())
            .map(|s| s.into())
            .collect())
    }

    fn is_gateway(&self) -> bool {
        self.info
            .rpc_methods
            .contains(&"sui_syncAccountState".to_string())
    }
}

impl SuiClient {
    pub async fn new_rpc_client(
        http_url: &str,
        ws_url: Option<&str>,
    ) -> Result<SuiClient, anyhow::Error> {
        let rpc = RpcClient::new(http_url, ws_url).await?;
        Ok(SuiClient::new(SuiClientApi::Rpc(rpc)))
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
        let wallet_sync_api = WalletSyncApi(api.clone());

        SuiClient {
            api,
            transaction_builder,
            read_api,
            full_node_api,
            event_api,
            quorum_driver,
            wallet_sync_api,
        }
    }

    pub fn is_gateway(&self) -> bool {
        match &*self.api {
            SuiClientApi::Rpc(c) => c.is_gateway(),
            SuiClientApi::Embedded(_) => true,
        }
    }

    pub fn available_rpc_methods(&self) -> Vec<String> {
        match &*self.api {
            SuiClientApi::Rpc(c) => c.info.rpc_methods.clone(),
            SuiClientApi::Embedded(_) => vec![],
        }
    }

    pub fn available_subscriptions(&self) -> Vec<String> {
        match &*self.api {
            SuiClientApi::Rpc(c) => c.info.subscriptions.clone(),
            SuiClientApi::Embedded(_) => vec![],
        }
    }

    pub fn api_version(&self) -> &str {
        match &*self.api {
            SuiClientApi::Rpc(c) => &c.info.version,
            SuiClientApi::Embedded(_) => env!("CARGO_PKG_VERSION"),
        }
    }

    pub fn check_api_version(&self) -> Result<(), anyhow::Error> {
        let server_version = self.api_version();
        let client_version = env!("CARGO_PKG_VERSION");
        if server_version != client_version {
            return Err(anyhow!("Client/Server api version mismatch, client api version : {client_version}, server api version : {server_version}"));
        };
        Ok(())
    }
}

#[derive(Debug)]
pub struct ReadApi {
    api: Arc<SuiClientApi>,
}

impl ReadApi {
    pub async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> anyhow::Result<Vec<SuiObjectInfo>> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c) => c.http.get_objects_owned_by_address(address).await?,
            SuiClientApi::Embedded(c) => c.get_objects_owned_by_address(address).await?,
        })
    }

    pub async fn get_objects_owned_by_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<Vec<SuiObjectInfo>> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c) => c.http.get_objects_owned_by_object(object_id).await?,
            SuiClientApi::Embedded(c) => c.get_objects_owned_by_object(object_id).await?,
        })
    }

    pub async fn get_parsed_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<GetObjectDataResponse> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c) => c.http.get_object(object_id).await?,
            SuiClientApi::Embedded(c) => c.get_object(object_id).await?,
        })
    }

    pub async fn try_get_parsed_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> anyhow::Result<GetPastObjectDataResponse> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c) => c.http.try_get_past_object(object_id, version).await?,
            // Gateway does not support get past object
            SuiClientApi::Embedded(_) => {
                unimplemented!("Gateway/embedded client does not support get past object")
            }
        })
    }

    pub async fn get_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<GetRawObjectDataResponse> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c) => c.http.get_raw_object(object_id).await?,
            SuiClientApi::Embedded(c) => c.get_raw_object(object_id).await?,
        })
    }

    pub async fn get_total_transaction_number(&self) -> anyhow::Result<u64> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c) => c.http.get_total_transaction_number().await?,
            SuiClientApi::Embedded(c) => c.get_total_transaction_number()?,
        })
    }

    pub async fn get_transactions_in_range(
        &self,
        start: TxSeqNumber,
        end: TxSeqNumber,
    ) -> anyhow::Result<Vec<TransactionDigest>> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c) => c.http.get_transactions_in_range(start, end).await?,
            SuiClientApi::Embedded(c) => c.get_transactions_in_range(start, end)?,
        })
    }

    pub async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> anyhow::Result<SuiTransactionResponse> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c) => c.http.get_transaction(digest).await?,
            SuiClientApi::Embedded(c) => c.get_transaction(digest).await?,
        })
    }

    pub async fn get_committee_info(
        &self,
        epoch: Option<EpochId>,
    ) -> anyhow::Result<CommitteeInfoResponse> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c) => c.http.get_committee_info(epoch).await?,
            SuiClientApi::Embedded(_c) => {
                unimplemented!("Gateway/embedded client does not support get committee info")
            }
        })
    }
}

#[derive(Clone)]
pub struct FullNodeApi(Arc<SuiClientApi>);

impl FullNodeApi {
    pub async fn get_transactions(
        &self,
        query: TransactionQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        order: Ordering,
    ) -> anyhow::Result<TransactionsPage> {
        Ok(match &*self.0 {
            SuiClientApi::Rpc(c) => c.http.get_transactions(query, cursor, limit, order).await?,
            SuiClientApi::Embedded(_) => {
                return Err(anyhow!("Method not supported by embedded gateway client."))
            }
        })
    }
}

#[derive(Clone)]
pub struct EventApi(Arc<SuiClientApi>);

impl EventApi {
    pub async fn subscribe_event(
        &self,
        filter: SuiEventFilter,
    ) -> anyhow::Result<impl Stream<Item = Result<SuiEventEnvelope, anyhow::Error>>> {
        match &*self.0 {
            SuiClientApi::Rpc(RpcClient { ws: Some(c), .. }) => {
                let subscription: Subscription<SuiEventEnvelope> =
                    c.subscribe_event(filter).await?;
                Ok(subscription.map(|item| Ok(item?)))
            }
            _ => Err(anyhow!("Subscription only supported by WebSocket client.")),
        }
    }
}

#[derive(Clone)]
pub struct QuorumDriver {
    api: Arc<SuiClientApi>,
}

impl QuorumDriver {
    /// Execute a transaction with a FullNode client or embedded Gateway.
    /// `request_type` is ignored when the client is an embedded Gateway.
    /// For Fullnode client, `request_type` defaults to
    /// `ExecuteTransactionRequestType::WaitForLocalExecution`.
    /// When `ExecuteTransactionRequestType::WaitForLocalExecution` is used,
    /// but returned `confirmed_local_execution` is false, the client polls
    /// the fullnode untils the fullnode recognizes this transaction, or
    /// until times out (see WAIT_FOR_TX_TIMEOUT_SEC). If it times out, an
    /// error is returned from this call.
    pub async fn execute_transaction(
        &self,
        tx: Transaction,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> anyhow::Result<TransactionExecutionResult> {
        Ok(match &*self.api {
            SuiClientApi::Rpc(c) => {
                let (tx_bytes, flag, signature, pub_key) = tx.to_network_data_for_execution();
                let request_type =
                    request_type.unwrap_or(ExecuteTransactionRequestType::WaitForLocalExecution);
                let resp = TransactionExecutionApiClient::execute_transaction(
                    &c.http,
                    tx_bytes,
                    flag,
                    signature,
                    pub_key,
                    request_type.clone(),
                )
                .await?;

                match (request_type, resp) {
                    (
                        ExecuteTransactionRequestType::ImmediateReturn,
                        SuiExecuteTransactionResponse::ImmediateReturn { tx_digest },
                    ) => TransactionExecutionResult {
                        tx_digest,
                        tx_cert: None,
                        effects: None,
                        confirmed_local_execution: false,
                        timestamp_ms: None,
                        parsed_data: None,
                    },
                    (
                        ExecuteTransactionRequestType::WaitForTxCert,
                        SuiExecuteTransactionResponse::TxCert { certificate },
                    ) => TransactionExecutionResult {
                        tx_digest: certificate.transaction_digest,
                        tx_cert: Some(certificate),
                        effects: None,
                        confirmed_local_execution: false,
                        timestamp_ms: None,
                        parsed_data: None,
                    },
                    (
                        ExecuteTransactionRequestType::WaitForEffectsCert,
                        SuiExecuteTransactionResponse::EffectsCert {
                            certificate,
                            effects,
                            confirmed_local_execution,
                        },
                    ) => TransactionExecutionResult {
                        tx_digest: certificate.transaction_digest,
                        tx_cert: Some(certificate),
                        effects: Some(effects.effects),
                        confirmed_local_execution,
                        timestamp_ms: None,
                        parsed_data: None,
                    },
                    (
                        ExecuteTransactionRequestType::WaitForLocalExecution,
                        SuiExecuteTransactionResponse::EffectsCert {
                            certificate,
                            effects,
                            confirmed_local_execution,
                        },
                    ) => {
                        if !confirmed_local_execution {
                            Self::wait_until_fullnode_sees_tx(c, certificate.transaction_digest)
                                .await?;
                        }
                        TransactionExecutionResult {
                            tx_digest: certificate.transaction_digest,
                            tx_cert: Some(certificate),
                            effects: Some(effects.effects),
                            confirmed_local_execution,
                            timestamp_ms: None,
                            parsed_data: None,
                        }
                    }
                    (other_request_type, other_resp) => {
                        bail!(
                            "Invalid response type {:?} for request type: {:?}",
                            other_resp,
                            other_request_type
                        );
                    }
                }
            }
            // TODO do we want to support an embedded quorum driver?
            SuiClientApi::Embedded(c) => {
                let resp = c.execute_transaction(tx).await?;
                TransactionExecutionResult {
                    tx_digest: resp.certificate.transaction_digest,
                    tx_cert: Some(resp.certificate),
                    effects: Some(resp.effects),
                    confirmed_local_execution: true,
                    timestamp_ms: resp.timestamp_ms,
                    parsed_data: resp.parsed_data,
                }
            }
        })
    }

    async fn wait_until_fullnode_sees_tx(
        c: &RpcClient,
        tx_digest: TransactionDigest,
    ) -> anyhow::Result<()> {
        let start = Instant::now();
        loop {
            let resp = RpcReadApiClient::get_transaction(&c.http, tx_digest).await;
            if let Err(err) = resp {
                if err.to_string().contains(TRANSACTION_NOT_FOUND_MSG_PREFIX) {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                } else {
                    // immediately return on other types of errors
                    bail!(
                        "Encountered error when confirming tx status for {:?}, err: {:?}",
                        tx_digest,
                        err
                    );
                }
            } else {
                return Ok(());
            }
            if start.elapsed().as_secs() >= WAIT_FOR_TX_TIMEOUT_SEC {
                bail!(
                    "Failed to confirm tx status for {:?} within {} seconds.",
                    tx_digest,
                    WAIT_FOR_TX_TIMEOUT_SEC
                );
            }
        }
    }
}

#[derive(Clone)]
pub struct WalletSyncApi(Arc<SuiClientApi>);

impl WalletSyncApi {
    pub async fn sync_account_state(&self, address: SuiAddress) -> anyhow::Result<()> {
        match &*self.0 {
            SuiClientApi::Rpc(_) => {
                unimplemented!("Rpc SuiClient does not support WalletSyncApi");
            }
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
                write!(
                    writer,
                    "Authorities : {:?}",
                    authorities.collect::<Vec<_>>()
                )?;
            }
            ClientType::RPC(url, ws_url) => {
                writeln!(writer, "Client Type : JSON-RPC")?;
                writeln!(writer, "HTTP RPC URL : {}", url)?;
                write!(
                    writer,
                    "WS RPC URL : {}",
                    ws_url.clone().unwrap_or_else(|| "None".to_string())
                )?;
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

#[async_trait]
impl DataReader for ReadApi {
    async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> Result<Vec<SuiObjectInfo>, anyhow::Error> {
        self.get_objects_owned_by_address(address).await
    }

    async fn get_object(
        &self,
        object_id: ObjectID,
    ) -> Result<GetRawObjectDataResponse, anyhow::Error> {
        self.get_object(object_id).await
    }
}
