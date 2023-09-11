// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::data_provider::DataProvider;
use crate::context_data::sui_sdk_data_provider::{lru_cache_data_loader, sui_sdk_client_v0};
use crate::extensions::logger::Logger;
use crate::extensions::timeout::Timeout;
use crate::server::builder::ServerBuilder;

use std::default::Default;

use super::builder::{DEFAULT_HOST, DEFAULT_PORT};

pub struct ServerConfig {
    pub port: u16,
    pub host: String,
    pub rpc_url: String,
}

impl std::default::Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            host: DEFAULT_HOST.to_string(),
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

pub async fn start_example_server(config: Option<ServerConfig>) {
    let config = config.unwrap_or_default();
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let sui_sdk_client_v0 = sui_sdk_client_v0(&config.rpc_url).await;
    let data_provider: Box<dyn DataProvider> = Box::new(sui_sdk_client_v0.clone());
    let data_loader = lru_cache_data_loader(&sui_sdk_client_v0).await;
    println!("Launch GraphiQL IDE at: {}", config.url());

    ServerBuilder::new()
        .port(config.port)
        .host(config.host)
        .context_data(data_provider)
        .context_data(data_loader)
        .extension(Logger::default())
        .extension(Timeout::default())
        .build()
        .run()
        .await;
}
