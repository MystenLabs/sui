// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use core::time::Duration;
use std::net::SocketAddr;
use sui_metrics_proxy::admin::{app, create_server_cert, manage_validators, server};
use sui_tls::TlsAcceptor;
use telemetry_subscribers::TelemetryConfig;
use tracing::info;

const GIT_REVISION: &str = {
    if let Some(revision) = option_env!("GIT_REVISION") {
        revision
    } else {
        let version = git_version::git_version!(
            args = ["--always", "--dirty", "--exclude", "*"],
            fallback = ""
        );

        if version.is_empty() {
            panic!("unable to query git revision");
        }
        version
    }
};
const VERSION: &str = const_str::concat!(env!("CARGO_PKG_VERSION"), "-", GIT_REVISION);

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
#[clap(name = env!("CARGO_BIN_NAME"))]
#[clap(version = VERSION)]
struct Args {
    #[clap(
        long,
        short,
        default_value = "localhost",
        help = "Specify the tls self-signed cert hostname to use"
    )]
    hostname: String,
    #[clap(long, short, help = "Specify the network name to use for labels")]
    network: String,
    #[clap(long, short, help = "Specify address to listen on")]
    listen_address: SocketAddr,
    #[clap(long, short, help = "Specify an upstream https url to send to")]
    upstream_address: String,
    #[clap(
        long,
        short,
        default_value = "http://localhost:9000",
        help = "Specify the rpc url to use when fetching our peer list for tls"
    )]
    rpc_url: String,
    #[clap(
        long,
        default_value_t = 30,
        help = "The poll interval (seconds) controls how often we verify our peer list"
    )]
    rpc_poll_interval: u64,
    #[clap(
        long,
        default_value_t = 10000,
        help = "mpsc buffer size - ideally set above our max rps"
    )]
    buffer_size: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let (_guard, _handle) = TelemetryConfig::new().init();
    let args = Args::parse();

    info!(
        "listen on {:?} send to {:?} using a buffered channel size of {}",
        args.listen_address, args.upstream_address, args.buffer_size
    );

    let listener = std::net::TcpListener::bind(args.listen_address).unwrap();

    let (tls_config, allowlist) =
        create_server_cert(&args.hostname).expect("unable to create self-signed server cert");
    let acceptor = TlsAcceptor::new(tls_config);

    manage_validators(
        args.rpc_url,
        Duration::from_secs(args.rpc_poll_interval),
        allowlist,
    );

    // create a multiple producer, single consumer channel
    // we use this to receive data from nodes and immediately return
    // StatusCode::OK. The http handlers will then process
    // it and send it upstream
    let app = app(args.buffer_size, args.network);
    server(listener, acceptor, app).await.unwrap();

    Ok(())
}
