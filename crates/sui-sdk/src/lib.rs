// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::{stream, StreamExt};
use futures_core::Stream;
use jsonrpsee::core::client::{ClientT, Subscription};
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};

use crate::error::{RpcError, SuiRpcResult};
use rpc_types::{
    GetPastObjectDataResponse, SuiCertifiedTransaction, SuiExecuteTransactionResponse,
    SuiParsedTransactionResponse, SuiTransactionEffects,
};
use serde_json::Value;
pub use sui_json as json;
use sui_json_rpc::api::CoinReadApiClient;
use sui_json_rpc::api::EventReadApiClient;
use sui_json_rpc::api::EventStreamingApiClient;
use sui_json_rpc::api::RpcBcsApiClient;
use sui_json_rpc::api::RpcFullNodeReadApiClient;
use sui_json_rpc::api::RpcReadApiClient;
use sui_json_rpc::api::TransactionExecutionApiClient;
pub use sui_json_rpc_types as rpc_types;
use sui_json_rpc_types::{
    Balance, Coin, CoinPage, EventPage, GetObjectDataResponse, GetRawObjectDataResponse,
    SuiCoinMetadata, SuiEventEnvelope, SuiEventFilter, SuiMoveNormalizedModule, SuiObjectInfo,
    SuiTransactionResponse, TransactionsPage,
};
use sui_transaction_builder::{DataReader, TransactionBuilder};
pub use sui_types as types;
use sui_types::balance::Supply;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::batch::TxSequenceNumber;
use sui_types::event::EventID;
use sui_types::messages::VerifiedTransaction;
use sui_types::query::{EventQuery, TransactionQuery};
use sui_types::sui_system_state::SuiSystemState;
use types::base_types::SequenceNumber;
use types::committee::EpochId;
use types::error::TRANSACTION_NOT_FOUND_MSG_PREFIX;
use types::messages::{CommitteeInfoResponse, ExecuteTransactionRequestType};
pub mod error;

pub const SUI_COIN_TYPE: &str = "0x2::sui::SUI";
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
    coin_read_api: CoinReadApi,
    event_api: EventApi,
    quorum_driver: QuorumDriver,
}

pub(crate) struct RpcClient {
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
    ) -> Result<Self, RpcError> {
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
    ) -> Result<ServerInfo, RpcError> {
        let rpc_spec: Value = http.request("rpc.discover", rpc_params![]).await?;
        let version = rpc_spec
            .pointer("/info/version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                RpcError::DataError(
                    "Fail parsing server version from rpc.discover endpoint.".into(),
                )
            })?;
        let rpc_methods = Self::parse_methods(&rpc_spec)?;

        let subscriptions = if let Some(ws) = ws {
            let rpc_spec: Value = ws.request("rpc.discover", rpc_params![]).await?;
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

    fn parse_methods(server_spec: &Value) -> Result<Vec<String>, RpcError> {
        let methods = server_spec
            .pointer("/methods")
            .and_then(|methods| methods.as_array())
            .ok_or_else(|| {
                RpcError::DataError(
                    "Fail parsing server information from rpc.discover endpoint.".into(),
                )
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
    ) -> Result<Self, RpcError> {
        let rpc = RpcClient::new(http_url, ws_url, request_timeout).await?;
        let api = Arc::new(rpc);
        let read_api = Arc::new(ReadApi::new(api.clone()));
        let quorum_driver = QuorumDriver::new(api.clone());
        let event_api = EventApi::new(api.clone());
        let transaction_builder = TransactionBuilder(read_api.clone());
        let coin_read_api = CoinReadApi::new(api.clone());

        Ok(SuiClient {
            api,
            transaction_builder,
            read_api,
            coin_read_api,
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

    pub fn check_api_version(&self) -> SuiRpcResult<()> {
        let server_version = self.api_version();
        let client_version = env!("CARGO_PKG_VERSION");
        if server_version != client_version {
            return Err(RpcError::ServerVersionMismatch {
                client_version: client_version.to_string(),
                server_version: server_version.to_string(),
            });
        };
        Ok(())
    }
}

#[derive(Debug)]
pub struct ReadApi {
    api: Arc<RpcClient>,
}

impl ReadApi {
    pub(crate) fn new(api: Arc<RpcClient>) -> Self {
        Self { api }
    }
    pub async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> SuiRpcResult<Vec<SuiObjectInfo>> {
        Ok(self.api.http.get_objects_owned_by_address(address).await?)
    }

    pub async fn get_objects_owned_by_object(
        &self,
        object_id: ObjectID,
    ) -> SuiRpcResult<Vec<SuiObjectInfo>> {
        Ok(self.api.http.get_objects_owned_by_object(object_id).await?)
    }

    pub async fn get_parsed_object(
        &self,
        object_id: ObjectID,
    ) -> SuiRpcResult<GetObjectDataResponse> {
        Ok(self.api.http.get_object(object_id).await?)
    }

    pub async fn try_get_parsed_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> SuiRpcResult<GetPastObjectDataResponse> {
        Ok(self
            .api
            .http
            .try_get_past_object(object_id, version)
            .await?)
    }

    pub async fn get_object(&self, object_id: ObjectID) -> SuiRpcResult<GetRawObjectDataResponse> {
        Ok(self.api.http.get_raw_object(object_id).await?)
    }

    pub async fn get_total_transaction_number(&self) -> SuiRpcResult<u64> {
        Ok(self.api.http.get_total_transaction_number().await?)
    }

    pub async fn get_transactions_in_range(
        &self,
        start: TxSequenceNumber,
        end: TxSequenceNumber,
    ) -> SuiRpcResult<Vec<TransactionDigest>> {
        Ok(self.api.http.get_transactions_in_range(start, end).await?)
    }

    pub async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> SuiRpcResult<SuiTransactionResponse> {
        Ok(self.api.http.get_transaction(digest).await?)
    }

    pub async fn get_committee_info(
        &self,
        epoch: Option<EpochId>,
    ) -> SuiRpcResult<CommitteeInfoResponse> {
        Ok(self.api.http.get_committee_info(epoch).await?)
    }

    pub async fn get_transactions(
        &self,
        query: TransactionQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> SuiRpcResult<TransactionsPage> {
        Ok(self
            .api
            .http
            .get_transactions(query, cursor, limit, Some(descending_order))
            .await?)
    }

    pub fn get_transactions_stream(
        &self,
        query: TransactionQuery,
        cursor: Option<TransactionDigest>,
        descending_order: bool,
    ) -> impl Stream<Item = TransactionDigest> + '_ {
        stream::unfold(
            (vec![], cursor, true, query),
            move |(mut data, cursor, first, query)| async move {
                if let Some(item) = data.pop() {
                    Some((item, (data, cursor, false, query)))
                } else if (cursor.is_none() && first) || cursor.is_some() {
                    let page = self
                        .get_transactions(query.clone(), cursor, Some(100), descending_order)
                        .await
                        .ok()?;
                    let mut data = page.data;
                    data.reverse();
                    data.pop()
                        .map(|item| (item, (data, page.next_cursor, false, query)))
                } else {
                    None
                }
            },
        )
    }

    pub async fn get_normalized_move_modules_by_package(
        &self,
        package: ObjectID,
    ) -> SuiRpcResult<BTreeMap<String, SuiMoveNormalizedModule>> {
        Ok(self
            .api
            .http
            .get_normalized_move_modules_by_package(package)
            .await?)
    }

    pub async fn get_sui_system_state(&self) -> SuiRpcResult<SuiSystemState> {
        Ok(self.api.http.get_sui_system_state().await?)
    }
}

#[derive(Debug, Clone)]
pub struct CoinReadApi {
    api: Arc<RpcClient>,
}

impl CoinReadApi {
    pub(crate) fn new(api: Arc<RpcClient>) -> Self {
        Self { api }
    }
    pub async fn get_coins(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> SuiRpcResult<CoinPage> {
        Ok(self
            .api
            .http
            .get_coins(owner, coin_type, cursor, limit)
            .await?)
    }

    pub fn get_coins_stream(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
    ) -> impl Stream<Item = Coin> + '_ {
        stream::unfold(
            (vec![], None, true, coin_type),
            move |(mut data, cursor, first, coin_type)| async move {
                if let Some(item) = data.pop() {
                    Some((item, (data, cursor, false, coin_type)))
                } else if (cursor.is_none() && first) || cursor.is_some() {
                    let page = self
                        .get_coins(owner, coin_type.clone(), cursor, Some(100))
                        .await
                        .ok()?;
                    let mut data = page.data;
                    data.reverse();
                    data.pop()
                        .map(|item| (item, (data, page.next_cursor, false, coin_type)))
                } else {
                    None
                }
            },
        )
    }

    pub async fn get_balances(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
    ) -> SuiRpcResult<Vec<Balance>> {
        Ok(self.api.http.get_balances(owner, coin_type).await?)
    }

    pub async fn get_coin_metadata(&self, coin_type: String) -> SuiRpcResult<SuiCoinMetadata> {
        Ok(self.api.http.get_coin_metadata(coin_type).await?)
    }

    pub async fn get_total_supply(&self, coin_type: String) -> SuiRpcResult<Supply> {
        Ok(self.api.http.get_total_supply(coin_type).await?)
    }
}

#[derive(Clone)]
pub struct EventApi {
    api: Arc<RpcClient>,
}

impl EventApi {
    pub(crate) fn new(api: Arc<RpcClient>) -> Self {
        Self { api }
    }
    pub async fn subscribe_event(
        &self,
        filter: SuiEventFilter,
    ) -> SuiRpcResult<impl Stream<Item = SuiRpcResult<SuiEventEnvelope>>> {
        match &self.api.ws {
            Some(c) => {
                let subscription: Subscription<SuiEventEnvelope> =
                    c.subscribe_event(filter).await?;
                Ok(subscription.map(|item| Ok(item?)))
            }
            _ => Err(RpcError::Subscription(
                "Subscription only supported by WebSocket client.".to_string(),
            )),
        }
    }

    pub async fn get_events(
        &self,
        query: EventQuery,
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> SuiRpcResult<EventPage> {
        Ok(self
            .api
            .http
            .get_events(query, cursor, limit, Some(descending_order))
            .await?)
    }

    pub fn get_events_stream(
        &self,
        query: EventQuery,
        cursor: Option<EventID>,
        descending_order: bool,
    ) -> impl Stream<Item = SuiEventEnvelope> + '_ {
        stream::unfold(
            (vec![], cursor, true, query),
            move |(mut data, cursor, first, query)| async move {
                if let Some(item) = data.pop() {
                    Some((item, (data, cursor, false, query)))
                } else if (cursor.is_none() && first) || cursor.is_some() {
                    let page = self
                        .get_events(query.clone(), cursor, Some(100), descending_order)
                        .await
                        .ok()?;
                    let mut data = page.data;
                    data.reverse();
                    data.pop()
                        .map(|item| (item, (data, page.next_cursor, false, query)))
                } else {
                    None
                }
            },
        )
    }
}

#[derive(Clone)]
pub struct QuorumDriver {
    api: Arc<RpcClient>,
}

impl QuorumDriver {
    pub(crate) fn new(api: Arc<RpcClient>) -> Self {
        Self { api }
    }
    /// Execute a transaction with a FullNode client. `request_type`
    /// defaults to `ExecuteTransactionRequestType::WaitForLocalExecution`.
    /// When `ExecuteTransactionRequestType::WaitForLocalExecution` is used,
    /// but returned `confirmed_local_execution` is false, the client polls
    /// the fullnode untils the fullnode recognizes this transaction, or
    /// until times out (see WAIT_FOR_TX_TIMEOUT_SEC). If it times out, an
    /// error is returned from this call.
    pub async fn execute_transaction(
        &self,
        tx: VerifiedTransaction,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> SuiRpcResult<TransactionExecutionResult> {
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
                return Err(RpcError::InvalidTransactionResponse(
                    other_resp,
                    other_request_type,
                ))
            }
        })
    }

    async fn wait_until_fullnode_sees_tx(
        c: &RpcClient,
        tx_digest: TransactionDigest,
    ) -> SuiRpcResult<()> {
        let start = Instant::now();
        loop {
            let resp = RpcReadApiClient::get_transaction(&c.http, tx_digest).await;
            if let Err(err) = resp {
                if err.to_string().contains(TRANSACTION_NOT_FOUND_MSG_PREFIX) {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                } else {
                    // immediately return on other types of errors
                    return Err(RpcError::TransactionConfirmationError(tx_digest, err));
                }
            } else {
                return Ok(());
            }
            if start.elapsed().as_secs() >= WAIT_FOR_TX_TIMEOUT_SEC {
                return Err(RpcError::FailToConfirmTransactionStatus(
                    tx_digest,
                    WAIT_FOR_TX_TIMEOUT_SEC,
                ));
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
    pub fn coin_read_api(&self) -> &CoinReadApi {
        &self.coin_read_api
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
        Ok(self.get_objects_owned_by_address(address).await?)
    }

    async fn get_object(
        &self,
        object_id: ObjectID,
    ) -> Result<GetRawObjectDataResponse, anyhow::Error> {
        Ok(self.get_object(object_id).await?)
    }
}
