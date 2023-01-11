// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::fmt::Formatter;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};

use crate::error::{Error, SuiRpcResult};
use rpc_types::{SuiCertifiedTransaction, SuiParsedTransactionResponse, SuiTransactionEffects};
use serde_json::Value;
use sui_adapter::execution_mode::Normal;
pub use sui_json as json;

use crate::apis::{CoinReadApi, EventApi, GovernanceApi, QuorumDriver, ReadApi};
pub use sui_json_rpc_types as rpc_types;
use sui_json_rpc_types::{GetRawObjectDataResponse, SuiObjectInfo};
use sui_transaction_builder::{DataReader, TransactionBuilder};
pub use sui_types as types;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};

pub mod apis;
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
    transaction_builder: TransactionBuilder<Normal>,
    read_api: Arc<ReadApi>,
    coin_read_api: CoinReadApi,
    event_api: EventApi,
    quorum_driver: QuorumDriver,
    governance_api: GovernanceApi,
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
    ) -> Result<Self, Error> {
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
    ) -> Result<ServerInfo, Error> {
        let rpc_spec: Value = http.request("rpc.discover", rpc_params![]).await?;
        let version = rpc_spec
            .pointer("/info/version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Error::DataError("Fail parsing server version from rpc.discover endpoint.".into())
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

    fn parse_methods(server_spec: &Value) -> Result<Vec<String>, Error> {
        let methods = server_spec
            .pointer("/methods")
            .and_then(|methods| methods.as_array())
            .ok_or_else(|| {
                Error::DataError(
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
    ) -> Result<Self, Error> {
        let rpc = RpcClient::new(http_url, ws_url, request_timeout).await?;
        let api = Arc::new(rpc);
        let read_api = Arc::new(ReadApi::new(api.clone()));
        let quorum_driver = QuorumDriver::new(api.clone());
        let event_api = EventApi::new(api.clone());
        let transaction_builder = TransactionBuilder::new(read_api.clone());
        let coin_read_api = CoinReadApi::new(api.clone());
        let governance_api = GovernanceApi::new(api.clone());

        Ok(SuiClient {
            api,
            transaction_builder,
            read_api,
            coin_read_api,
            event_api,
            quorum_driver,
            governance_api,
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
            return Err(Error::ServerVersionMismatch {
                client_version: client_version.to_string(),
                server_version: server_version.to_string(),
            });
        };
        Ok(())
    }
}

impl SuiClient {
    pub fn transaction_builder(&self) -> &TransactionBuilder<Normal> {
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
    pub fn governance_api(&self) -> &GovernanceApi {
        &self.governance_api
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
