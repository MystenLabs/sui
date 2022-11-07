// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

extern crate core;

use std::fmt::Debug;
use std::fmt::Formatter;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail};
use async_trait::async_trait;
use futures::StreamExt;
use futures_core::Stream;
use jsonrpsee::core::client::{ClientT, Subscription};
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};

use rpc_types::{
    GetPastObjectDataResponse, SuiCertifiedTransaction, SuiExecuteTransactionResponse,
    SuiParsedTransactionResponse, SuiTransactionEffects,
};
use serde_json::Value;
pub use sui_config::gateway;
use sui_core::gateway_state::TxSeqNumber;
pub use sui_json as json;
use sui_json_rpc::api::EventReadApiClient;
use sui_json_rpc::api::EventStreamingApiClient;
use sui_json_rpc::api::RpcBcsApiClient;
use sui_json_rpc::api::RpcFullNodeReadApiClient;
use sui_json_rpc::api::RpcReadApiClient;
use sui_json_rpc::api::TransactionExecutionApiClient;
pub use sui_json_rpc_types as rpc_types;
use sui_json_rpc_types::{
    EventPage, GetObjectDataResponse, GetRawObjectDataResponse, SuiEventEnvelope, SuiEventFilter,
    SuiObjectInfo, SuiTransactionResponse, TransactionsPage,
};
use sui_transaction_builder::{DataReader, TransactionBuilder};
pub use sui_types as types;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::event::EventID;
use sui_types::messages::VerifiedTransaction;
use sui_types::query::{EventQuery, TransactionQuery};
use types::base_types::SequenceNumber;
use types::committee::EpochId;
use types::error::TRANSACTION_NOT_FOUND_MSG_PREFIX;
use types::messages::{CommitteeInfoResponse, ExecuteTransactionRequestType};

#[cfg(msim)]
pub mod embedded_gateway;

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
    api: Arc<RpcClient>,
    transaction_builder: TransactionBuilder,
    read_api: Arc<ReadApi>,
    event_api: EventApi,
    quorum_driver: QuorumDriver,
}

struct RpcClient {
    http: HttpClient,
    ws: Option<WsClient>,
    info: ServerInfo,
}

impl Debug for RpcClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RPC client. Http: {:?}, Websocket: {:?}",
            self.http, self.ws
        )
    }
}

struct ServerInfo {
    rpc_methods: Vec<String>,
    subscriptions: Vec<String>,
    version: String,
}

impl RpcClient {
    pub async fn new(
        http: &str,
        ws: Option<&str>,
        request_timeout: Option<Duration>,
    ) -> Result<Self, anyhow::Error> {
        let mut http_builder = HttpClientBuilder::default();
        if let Some(request_timeout) = request_timeout {
            http_builder = http_builder.request_timeout(request_timeout);
        }
        let http = http_builder.build(http)?;

        let ws = if let Some(url) = ws {
            let mut ws_builder = WsClientBuilder::default();
            if let Some(request_timeout) = request_timeout {
                ws_builder = ws_builder.request_timeout(request_timeout);
            }
            let ws = ws_builder.build(url).await?;
            Some(ws)
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
}

impl SuiClient {
    pub async fn new(
        http_url: &str,
        ws_url: Option<&str>,
        request_timeout: Option<Duration>,
    ) -> Result<Self, anyhow::Error> {
        let rpc = RpcClient::new(http_url, ws_url, request_timeout).await?;
        let api = Arc::new(rpc);
        let read_api = Arc::new(ReadApi { api: api.clone() });
        let quorum_driver = QuorumDriver { api: api.clone() };
        let event_api = EventApi(api.clone());
        let transaction_builder = TransactionBuilder(read_api.clone());

        Ok(SuiClient {
            api,
            transaction_builder,
            read_api,
            event_api,
            quorum_driver,
        })
    }

    pub fn available_rpc_methods(&self) -> &Vec<String> {
        &self.api.info.rpc_methods
    }

    pub fn available_subscriptions(&self) -> &Vec<String> {
        &self.api.info.subscriptions
    }

    pub fn api_version(&self) -> &str {
        &self.api.info.version
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
    api: Arc<RpcClient>,
}

impl ReadApi {
    pub async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> anyhow::Result<Vec<SuiObjectInfo>> {
        Ok(self.api.http.get_objects_owned_by_address(address).await?)
    }

    pub async fn get_objects_owned_by_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<Vec<SuiObjectInfo>> {
        Ok(self.api.http.get_objects_owned_by_object(object_id).await?)
    }

    pub async fn get_parsed_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<GetObjectDataResponse> {
        Ok(self.api.http.get_object(object_id).await?)
    }

    pub async fn try_get_parsed_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> anyhow::Result<GetPastObjectDataResponse> {
        Ok(self
            .api
            .http
            .try_get_past_object(object_id, version)
            .await?)
    }

    pub async fn get_object(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<GetRawObjectDataResponse> {
        Ok(self.api.http.get_raw_object(object_id).await?)
    }

    pub async fn get_total_transaction_number(&self) -> anyhow::Result<u64> {
        Ok(self.api.http.get_total_transaction_number().await?)
    }

    pub async fn get_transactions_in_range(
        &self,
        start: TxSeqNumber,
        end: TxSeqNumber,
    ) -> anyhow::Result<Vec<TransactionDigest>> {
        Ok(self.api.http.get_transactions_in_range(start, end).await?)
    }

    pub async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> anyhow::Result<SuiTransactionResponse> {
        Ok(self.api.http.get_transaction(digest).await?)
    }

    pub async fn get_committee_info(
        &self,
        epoch: Option<EpochId>,
    ) -> anyhow::Result<CommitteeInfoResponse> {
        Ok(self.api.http.get_committee_info(epoch).await?)
    }

    pub async fn get_transactions(
        &self,
        query: TransactionQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> anyhow::Result<TransactionsPage> {
        Ok(self
            .api
            .http
            .get_transactions(query, cursor, limit, descending_order)
            .await?)
    }
}

#[derive(Clone)]
pub struct EventApi(Arc<RpcClient>);

impl EventApi {
    pub async fn subscribe_event(
        &self,
        filter: SuiEventFilter,
    ) -> anyhow::Result<impl Stream<Item = Result<SuiEventEnvelope, anyhow::Error>>> {
        match &self.0.ws {
            Some(c) => {
                let subscription: Subscription<SuiEventEnvelope> =
                    c.subscribe_event(filter).await?;
                Ok(subscription.map(|item| Ok(item?)))
            }
            _ => Err(anyhow!("Subscription only supported by WebSocket client.")),
        }
    }

    pub async fn get_events(
        &self,
        query: EventQuery,
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> anyhow::Result<EventPage> {
        Ok(self
            .0
            .http
            .get_events(query, cursor, limit, descending_order)
            .await?)
    }
}

#[derive(Clone)]
pub struct QuorumDriver {
    api: Arc<RpcClient>,
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
        tx: VerifiedTransaction,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> anyhow::Result<TransactionExecutionResult> {
        let (tx_bytes, flag, signature, pub_key) = tx.to_network_data_for_execution();
        let request_type =
            request_type.unwrap_or(ExecuteTransactionRequestType::WaitForLocalExecution);
        let resp = TransactionExecutionApiClient::execute_transaction(
            &self.api.http,
            tx_bytes,
            flag,
            signature,
            pub_key,
            request_type.clone(),
        )
        .await?;

        Ok(match (request_type, resp) {
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
                    Self::wait_until_fullnode_sees_tx(&self.api, certificate.transaction_digest)
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

impl SuiClient {
    pub fn transaction_builder(&self) -> &TransactionBuilder {
        &self.transaction_builder
    }
    pub fn read_api(&self) -> &ReadApi {
        &self.read_api
    }
    pub fn event_api(&self) -> &EventApi {
        &self.event_api
    }
    pub fn quorum_driver(&self) -> &QuorumDriver {
        &self.quorum_driver
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
