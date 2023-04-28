// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::fmt::Formatter;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::http_client::{HeaderMap, HeaderValue, HttpClient, HttpClientBuilder};
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use serde_json::Value;

use move_core_types::language_storage::StructTag;
pub use sui_json as json;
use sui_json_rpc::{
    CLIENT_SDK_TYPE_HEADER, CLIENT_SDK_VERSION_HEADER, CLIENT_TARGET_API_VERSION_HEADER,
};
pub use sui_json_rpc_types as rpc_types;
use sui_json_rpc_types::{
    ObjectsPage, SuiObjectDataFilter, SuiObjectDataOptions, SuiObjectResponse,
    SuiObjectResponseQuery,
};
use sui_transaction_builder::{DataReader, TransactionBuilder};
pub use sui_types as types;
use sui_types::base_types::{ObjectID, ObjectInfo, SuiAddress};

use crate::apis::{CoinReadApi, EventApi, GovernanceApi, QuorumDriverApi, ReadApi};
use crate::error::{Error, SuiRpcResult};

pub mod apis;
pub mod error;
pub mod sui_client_config;
pub mod wallet_context;
pub const SUI_COIN_TYPE: &str = "0x2::sui::SUI";
const WAIT_FOR_TX_TIMEOUT_SEC: u64 = 60;

pub struct SuiClientBuilder {
    request_timeout: Duration,
    max_concurrent_requests: usize,
    ws_url: Option<String>,
}

impl Default for SuiClientBuilder {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(60),
            max_concurrent_requests: 256,
            ws_url: None,
        }
    }
}

impl SuiClientBuilder {
    pub fn request_timeout(mut self, request_timeout: Duration) -> Self {
        self.request_timeout = request_timeout;
        self
    }

    pub fn max_concurrent_requests(mut self, max_concurrent_requests: usize) -> Self {
        self.max_concurrent_requests = max_concurrent_requests;
        self
    }

    pub fn ws_url(mut self, url: impl AsRef<str>) -> Self {
        self.ws_url = Some(url.as_ref().to_string());
        self
    }

    pub async fn build(self, http: impl AsRef<str>) -> SuiRpcResult<SuiClient> {
        let client_version = env!("CARGO_PKG_VERSION");
        let mut headers = HeaderMap::new();
        headers.insert(
            CLIENT_TARGET_API_VERSION_HEADER,
            // for rust, the client version is the same as the target api version
            HeaderValue::from_static(client_version),
        );
        headers.insert(
            CLIENT_SDK_VERSION_HEADER,
            HeaderValue::from_static(client_version),
        );
        headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("rust"));

        let ws = if let Some(url) = self.ws_url {
            Some(
                WsClientBuilder::default()
                    .max_request_body_size(2 << 30)
                    .max_concurrent_requests(self.max_concurrent_requests)
                    .set_headers(headers.clone())
                    .request_timeout(self.request_timeout)
                    .build(url)
                    .await?,
            )
        } else {
            None
        };

        let http = HttpClientBuilder::default()
            .max_request_body_size(2 << 30)
            .max_concurrent_requests(self.max_concurrent_requests)
            .set_headers(headers.clone())
            .request_timeout(self.request_timeout)
            .build(http)?;

        let info = Self::get_server_info(&http, &ws).await?;

        let rpc = RpcClient { http, ws, info };
        let api = Arc::new(rpc);
        let read_api = Arc::new(ReadApi::new(api.clone()));
        let quorum_driver_api = QuorumDriverApi::new(api.clone());
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
            quorum_driver_api,
            governance_api,
        })
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

/// Use [SuiClientBuilder] to build a SuiClient
#[derive(Clone)]
pub struct SuiClient {
    api: Arc<RpcClient>,
    transaction_builder: TransactionBuilder,
    read_api: Arc<ReadApi>,
    coin_read_api: CoinReadApi,
    event_api: EventApi,
    quorum_driver_api: QuorumDriverApi,
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

impl SuiClient {
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
    pub fn quorum_driver_api(&self) -> &QuorumDriverApi {
        &self.quorum_driver_api
    }
    pub fn governance_api(&self) -> &GovernanceApi {
        &self.governance_api
    }
}

#[async_trait]
impl DataReader for ReadApi {
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        object_type: StructTag,
    ) -> Result<Vec<ObjectInfo>, anyhow::Error> {
        let mut result = vec![];
        let query = Some(SuiObjectResponseQuery {
            filter: Some(SuiObjectDataFilter::StructType(object_type)),
            options: Some(
                SuiObjectDataOptions::new()
                    .with_previous_transaction()
                    .with_type()
                    .with_owner(),
            ),
        });

        let mut has_next = true;
        let mut cursor = None;

        while has_next {
            let ObjectsPage {
                data,
                next_cursor,
                has_next_page,
            } = self
                .get_owned_objects(address, query.clone(), cursor, None)
                .await?;
            result.extend(
                data.iter()
                    .map(|r| r.clone().try_into())
                    .collect::<Result<Vec<_>, _>>()?,
            );
            cursor = next_cursor;
            has_next = has_next_page;
        }
        Ok(result)
    }

    async fn get_object_with_options(
        &self,
        object_id: ObjectID,
        options: SuiObjectDataOptions,
    ) -> Result<SuiObjectResponse, anyhow::Error> {
        Ok(self.get_object_with_options(object_id, options).await?)
    }

    async fn get_reference_gas_price(&self) -> Result<u64, anyhow::Error> {
        Ok(self.get_reference_gas_price().await?)
    }
}
