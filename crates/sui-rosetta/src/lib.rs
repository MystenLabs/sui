// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::post;
use axum::{Extension, Router};
use once_cell::sync::Lazy;
use sui_config::genesis::Genesis;
use sui_core::authority::AuthorityState;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_quorum_driver::QuorumDriver;
use tokio::task::JoinHandle;
use tracing::info;

use crate::errors::{Error, ErrorType};
use crate::state::{BookKeeper, PseudoBlockProvider, ServerContext};
use crate::types::{Currency, NetworkIdentifier, SuiEnv};
use crate::ErrorType::{UnsupportedBlockchain, UnsupportedNetwork};

mod account;
mod block;
mod construction;
mod errors;
mod network;
mod operations;
mod state;
mod types;

pub static SUI: Lazy<Currency> = Lazy::new(|| Currency {
    symbol: "SUI".to_string(),
    decimals: 8,
});

pub struct RosettaServer {
    context: ServerContext,
}

impl RosettaServer {
    pub fn new(
        state: Arc<AuthorityState>,
        quorum_driver: Arc<QuorumDriver<NetworkAuthorityClient>>,
        genesis: &Genesis,
    ) -> Self {
        let blocks = Arc::new(PseudoBlockProvider::spawn(state.clone(), genesis));
        Self {
            context: ServerContext::new(
                SuiEnv::MainNet,
                state.clone(),
                quorum_driver,
                blocks.clone(),
                BookKeeper { state, blocks },
            ),
        }
    }

    pub fn serve(self, addr: SocketAddr) -> JoinHandle<hyper::Result<()>> {
        let app = Router::new()
            .route("/account/balance", post(account::balance))
            .route("/account/coins", post(account::coins))
            .route("/block", post(block::block))
            .route("/block/transaction", post(block::transaction))
            .route("/construction/derive", post(construction::derive))
            .route("/construction/payload", post(construction::payload))
            .route("/construction/combine", post(construction::combine))
            .route("/construction/submit", post(construction::submit))
            .route("/construction/preprocess", post(construction::preprocess))
            .route("/construction/hash", post(construction::hash))
            .route("/construction/metadata", post(construction::metadata))
            .route("/construction/parse", post(construction::parse))
            .route("/network/list", post(network::list))
            .route("/network/status", post(network::status))
            .route("/network/options", post(network::options))
            .layer(Extension(Arc::new(self.context)));
        let server = axum::Server::bind(&addr).serve(app.into_make_service());
        info!("Sui Rosetta server listening on {}", server.local_addr());
        tokio::spawn(server)
    }
}
