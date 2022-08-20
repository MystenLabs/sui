// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::routing::post;
use axum::{Extension, Router};
use itertools::Itertools;
use once_cell::sync::Lazy;
use serde_json::json;
use tracing::info;

use sui_sdk::SuiClient;

use crate::errors::{Error, ErrorType};
use crate::types::{Currency, NetworkIdentifier, SuiEnv};
use crate::ErrorType::{UnsupportedBlockchain, UnsupportedNetwork};

mod account;
mod actions;
mod construction;
mod errors;
mod network;
mod types;

pub static SUI: Lazy<Currency> = Lazy::new(|| Currency {
    symbol: "SUI".to_string(),
    decimals: 8,
});

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // initialize tracing
    let _guard = telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
        .with_env()
        .init();

    let state = Arc::new(ApiState {
        clients: BTreeMap::from_iter(vec![(
            SuiEnv::MainNet,
            SuiClient::new_rpc_client("http://127.0.0.1:9000", None).await?,
        )]),
    });

    let app = Router::new()
        .route("/account/balance", post(account::balance))
        .route("/account/coins", post(account::coins))
        .route("/network/list", post(network::list))
        .route("/network/status", post(network::status))
        .route("/network/options", post(network::options))
        .route("/construction/derive", post(construction::derive))
        .route("/construction/payload", post(construction::payload))
        .route("/construction/combine", post(construction::combine))
        .route("/construction/submit", post(construction::submit))
        .route("/construction/preprocess", post(construction::preprocess))
        .route("/construction/hash", post(construction::hash))
        .route("/construction/metadata", post(construction::metadata))
        .route("/construction/parse", post(construction::parse))
        .layer(Extension(state));

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9002);
    info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

pub struct ApiState {
    clients: BTreeMap<SuiEnv, SuiClient>,
}

impl ApiState {
    pub async fn get_client(&self, env: SuiEnv) -> Result<&SuiClient, Error> {
        self.clients.get(&env).ok_or_else(|| {
            Error::new_with_detail(ErrorType::UnsupportedNetwork, json!({ "env": env }))
        })
    }

    pub fn get_envs(&self) -> Vec<SuiEnv> {
        self.clients.keys().cloned().collect()
    }

    fn checks_network_identifier(
        &self,
        network_identifier: &NetworkIdentifier,
    ) -> Result<(), Error> {
        if &network_identifier.blockchain != "sui" {
            return Err(Error::new(UnsupportedBlockchain));
        }
        if !self.clients.keys().contains(&network_identifier.network) {
            return Err(Error::new(UnsupportedNetwork));
        }
        Ok(())
    }
}
