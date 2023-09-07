// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::extensions::logger::Logger;
use crate::extensions::timeout::Timeout;
use crate::server::sui_sdk_data_provider::SuiClientLoader;
use crate::{
    server::{
        data_provider::DataProvider,
        version::{check_version_middleware, set_version_middleware},
    },
    types::query::{Query, SuiGraphQLSchema},
};
use async_graphql::dataloader::DataLoader;
use async_graphql::{EmptyMutation, EmptySubscription};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::middleware;
use std::default::Default;
use std::time::Duration;

pub(crate) const RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD: Duration = Duration::from_millis(10_000);
pub(crate) const MAX_CONCURRENT_REQUESTS: usize = 1_000;
pub(crate) const DATA_LOADER_LRU_CACHE_SIZE: usize = 1_000;

pub struct ServerConfig {
    pub port: u16,
    pub host: String,
    pub rpc_url: String,
}

impl std::default::Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 8000,
            host: "127.0.0.1".to_string(),
            rpc_url: "https://fullnode.testnet.sui.io:443/".to_string(),
        }
    }
}

impl ServerConfig {
    pub fn url(&self) -> String {
        format!("http://{}", self.address())
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

async fn graphql_handler(
    schema: axum::Extension<SuiGraphQLSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

async fn graphiql() -> impl axum::response::IntoResponse {
    axum::response::Html(
        async_graphql::http::GraphiQLSource::build()
            .endpoint("/")
            .finish(),
    )
}

pub async fn start_example_server(config: Option<ServerConfig>) {
    let config = config.unwrap_or_default();
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let sui_sdk_client_v0 = sui_sdk::SuiClientBuilder::default()
        .request_timeout(RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD)
        .max_concurrent_requests(MAX_CONCURRENT_REQUESTS)
        .build(config.rpc_url.as_str())
        .await
        .expect("Failed to create SuiClient");

    let data_provider: Box<dyn DataProvider> = Box::new(sui_sdk_client_v0.clone());
    let data_loader = DataLoader::with_cache(
        SuiClientLoader {
            client: sui_sdk_client_v0,
        },
        tokio::spawn,
        async_graphql::dataloader::LruCache::new(DATA_LOADER_LRU_CACHE_SIZE),
    );
    data_loader.enable_all_cache(true);
    let schema = async_graphql::Schema::build(Query, EmptyMutation, EmptySubscription)
        .data(data_provider)
        .data(data_loader)
        .extension(Logger::default())
        .extension(Timeout::default())
        .finish();

    let app = axum::Router::new()
        .route("/", axum::routing::get(graphiql).post(graphql_handler))
        .layer(axum::extract::Extension(schema))
        .layer(middleware::from_fn(check_version_middleware))
        .layer(middleware::from_fn(set_version_middleware));

    println!("Launch GraphiQL IDE at: {}", config.url());

    axum::Server::bind(&config.address().parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
