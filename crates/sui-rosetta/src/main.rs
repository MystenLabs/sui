// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::routing::post;
use axum::{Extension, Router};
use lazy_static::lazy_static;
use serde_json::json;
use tower::ServiceBuilder;
use tracing::info;

use sui_sdk::SuiClient;

use crate::errors::{Error, ErrorType};
use crate::types::{Currency, SuiEnv};

mod account;
mod actions;
mod construction;
mod errors;
mod network;
mod types;

lazy_static! {
    pub static ref SUI: Currency = Currency {
        symbol: "SUI".to_string(),
        decimals: 8,
    };
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let client = Arc::new(ApiState {
        clients: BTreeMap::from_iter(vec![(
            SuiEnv::MainNet,
            SuiClient::new_rpc_client("http://127.0.0.1:9000", None).await?,
        )]),
    });

    let app = Router::new()
        .route("/account/balance", post(account::balance))
        .route("/account/coins", post(account::coins))
        .route("/network/list", post(network::list))
        .route("/construction/derive", post(construction::derive))
        .route("/construction/payload", post(construction::payload))
        .route("/construction/combine", post(construction::combine))
        .route("/construction/submit", post(construction::submit))
        .route("/construction/preprocess", post(construction::preprocess))
        .layer(
            ServiceBuilder::new()
                //.layer(HandleErrorLayer::new(handle_error))
                .layer(Extension(client))
                .into_inner(),
        );

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
}
