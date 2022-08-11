// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use axum::{response::IntoResponse, routing::post, Extension, Json, Router};
use clap::Parser;
use http::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use sui_cluster_test::{
    cluster::{Cluster, LocalNewCluster},
    config::{ClusterTestOpt, Env},
    faucet::{FaucetClient, FaucetClientFactory},
    wallet_client::WalletClient,
};
use sui_types::base_types::SuiAddress;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};

/// Start a Sui validator and fullnode for easy testing.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    // TODO: Support a configuration directory for persisted networks:
    // /// Config directory that will be used to store network configuration
    // #[clap(short, long, parse(from_os_str), value_hint = ValueHint::DirPath)]
    // config: Option<std::path::PathBuf>,
    /// Port to start the Gateway RPC server on
    #[clap(long, default_value = "5001")]
    gateway_rpc_port: u16,

    // Port to start the Sui faucet on
    #[clap(long)]
    faucet_port: Option<u16>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let cluster = LocalNewCluster::start(&ClusterTestOpt {
        env: Env::NewLocal,
        gateway_address: Some(format!("127.0.0.1:{}", args.gateway_rpc_port)),
        fullnode_address: None,
        faucet_address: None,
    })
    .await?;

    println!("Gateway RPC URL: {}", cluster.rpc_url());

    if let Some(faucet_port) = args.faucet_port {
        start_faucet(&cluster, faucet_port).await?;
    } else {
        // Perform health checks to keep the service running:
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            for node in cluster.swarm().validators() {
                node.health_check().await.unwrap();
            }

            interval.tick().await;
        }
    }

    Ok(())
}

struct AppState {
    faucet: Arc<dyn FaucetClient + Sync + Send>,
    wallet_client: WalletClient,
}

async fn start_faucet(cluster: &LocalNewCluster, port: u16) -> Result<()> {
    let wallet_client = WalletClient::new_from_cluster(cluster).await;
    let faucet = FaucetClientFactory::new_from_cluster(cluster).await;

    let app_state = Arc::new(AppState {
        faucet,
        wallet_client,
    });

    let cors = CorsLayer::new()
        .allow_methods(vec![Method::GET, Method::POST])
        .allow_headers(Any)
        .allow_origin(Any);

    let app = Router::new().route("/faucet", post(faucet_request)).layer(
        ServiceBuilder::new()
            .layer(cors)
            .layer(Extension(app_state))
            .into_inner(),
    );

    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    println!("Faucet listening on http://{}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct FaucetRequest {
    pub recipient: SuiAddress,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct FaucetResponse {
    pub ok: bool,
}

async fn faucet_request(
    Json(payload): Json<FaucetRequest>,
    Extension(state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    let result = state
        .faucet
        .request_sui_coins(&state.wallet_client, Some(1), Some(payload.recipient))
        .await;

    match result {
        Ok(_) => (StatusCode::OK, Json(FaucetResponse { ok: true })),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(FaucetResponse { ok: false }),
        ),
    }
}
