// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::post;
use axum::{Extension, Router};
use once_cell::sync::Lazy;
use tokio::task::JoinHandle;
use tracing::info;

use sui_config::genesis::Genesis;
use sui_core::authority::AuthorityState;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_quorum_driver::QuorumDriver;

use crate::errors::{Error, ErrorType};
use crate::state::{OnlineServerContext, PseudoBlockProvider};
use crate::types::{Currency, SuiEnv};
use crate::ErrorType::{UnsupportedBlockchain, UnsupportedNetwork};

mod account;
mod block;
mod construction;
mod errors;
mod network;
mod operations;
mod state;
pub mod types;

pub static SUI: Lazy<Currency> = Lazy::new(|| Currency {
    symbol: "SUI".to_string(),
    decimals: 9,
});

pub struct RosettaOnlineServer {
    env: SuiEnv,
    context: OnlineServerContext,
}

impl RosettaOnlineServer {
    pub fn new(
        env: SuiEnv,
        state: Arc<AuthorityState>,
        quorum_driver: Arc<QuorumDriver<NetworkAuthorityClient>>,
        genesis: &Genesis,
    ) -> Self {
        let blocks = Arc::new(PseudoBlockProvider::spawn(state.clone(), genesis));
        Self {
            env,
            context: OnlineServerContext::new(state, quorum_driver, blocks),
        }
    }

    pub fn serve(self, addr: SocketAddr) -> JoinHandle<hyper::Result<()>> {
        // Online endpoints
        let app = Router::new()
            .route("/account/balance", post(account::balance))
            .route("/account/coins", post(account::coins))
            .route("/block", post(block::block))
            .route("/block/transaction", post(block::transaction))
            .route("/construction/submit", post(construction::submit))
            .route("/construction/metadata", post(construction::metadata))
            .route("/construction/parse", post(construction::parse))
            .route("/network/status", post(network::status))
            .route("/network/list", post(network::list))
            .route("/network/options", post(network::options))
            .layer(Extension(self.env))
            .layer(Extension(Arc::new(self.context)));
        let server = axum::Server::bind(&addr).serve(app.into_make_service());
        info!(
            "Sui Rosetta online server listening on {}",
            server.local_addr()
        );
        tokio::spawn(server)
    }
}

pub struct RosettaOfflineServer {
    env: SuiEnv,
}

impl RosettaOfflineServer {
    pub fn new(env: SuiEnv) -> Self {
        Self { env }
    }

    pub fn serve(self, addr: SocketAddr) -> JoinHandle<hyper::Result<()>> {
        // Online endpoints
        let app = Router::new()
            .route("/construction/derive", post(construction::derive))
            .route("/construction/payloads", post(construction::payloads))
            .route("/construction/combine", post(construction::combine))
            .route("/construction/preprocess", post(construction::preprocess))
            .route("/construction/hash", post(construction::hash))
            .route("/construction/parse", post(construction::parse))
            .route("/network/list", post(network::list))
            .route("/network/options", post(network::options))
            .layer(Extension(self.env));
        let server = axum::Server::bind(&addr).serve(app.into_make_service());
        info!(
            "Sui Rosetta offline server listening on {}",
            server.local_addr()
        );
        tokio::spawn(server)
    }
}

#[test]
fn get_key() {
    use std::collections::BTreeMap;
    use std::fs::File;
    use std::io::BufReader;
    use sui_config::{sui_config_dir, SUI_KEYSTORE_FILENAME};
    use sui_types::base_types::SuiAddress;
    use sui_types::crypto::{EncodeDecodeBase64, KeypairTraits, SuiKeyPair, ToFromBytes};
    use sui_types::sui_serde::{Encoding, Hex};

    let path = sui_config_dir().unwrap().join(SUI_KEYSTORE_FILENAME);

    let reader = BufReader::new(File::open(path).unwrap());
    let kp_strings: Vec<String> = serde_json::from_reader(reader).unwrap();
    let keys = kp_strings
        .iter()
        .map(|kpstr| {
            let key = SuiKeyPair::decode_base64(kpstr);
            key.map(|k| (Into::<SuiAddress>::into(&k.public()), k))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
        .unwrap();

    for (address, key) in keys {
        match key {
            SuiKeyPair::Ed25519SuiKeyPair(k) => {
                println!(
                    "{}: {}: ed25519",
                    address,
                    Hex::encode(k.private().as_bytes())
                )
            }
            SuiKeyPair::Secp256k1SuiKeyPair(k) => {
                println!(
                    "{}: {}: secp256k1",
                    address,
                    Hex::encode(k.private().as_bytes())
                )
            }
        };
    }
}
