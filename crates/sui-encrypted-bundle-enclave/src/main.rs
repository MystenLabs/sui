// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use clap::Parser;
use sui_encrypted_bundle_enclave::{EnclaveState, MockAttestor, router};
use tracing::info;

#[derive(Parser)]
#[command(about = "Builds encrypted transaction bundles for a validator")]
struct Args {
    #[arg(long, default_value_t = 3000)]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
    let args = Args::parse();
    // A Nitro build replaces the mock with an attestor that calls the NSM API.
    let state = Arc::new(EnclaveState::new(
        &mut rand::thread_rng(),
        Box::new(MockAttestor),
    ));
    let address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, args.port));
    let listener = tokio::net::TcpListener::bind(address).await?;
    info!("encrypted bundle enclave listening on {address}");
    axum::serve(listener, router(state)).await?;
    Ok(())
}
