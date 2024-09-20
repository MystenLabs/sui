// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::Result;
use clap::Parser;
use std::env;
use sui_proxy::config::ProxyConfig;
use sui_proxy::{
    admin::{
        app, create_server_cert_default_allow, create_server_cert_enforce_peer,
        make_reqwest_client, server, Labels,
    },
    config::load,
    histogram_relay, metrics,
};
use sui_tls::TlsAcceptor;
use telemetry_subscribers::TelemetryConfig;
use tracing::info;

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

/// user agent we use when posting to mimir
static APP_USER_AGENT: &str = const_str::concat!(
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
    "/",
    VERSION
);

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
#[clap(name = env!("CARGO_BIN_NAME"))]
#[clap(version = VERSION)]
struct Args {
    #[clap(
        long,
        short,
        default_value = "./sui-proxy.yaml",
        help = "Specify the config file path to use"
    )]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let (_guard, _handle) = TelemetryConfig::new().init();

    let args = Args::parse();

    let config: ProxyConfig = load(args.config)?;

    info!(
        "listen on {:?} send to {:?}",
        config.listen_address, config.remote_write.url
    );

    let listener = std::net::TcpListener::bind(config.listen_address).unwrap();

    let (tls_config, allower) =
        // we'll only use the dynamic peers in some cases - it makes little sense to run with the static's
        // since this first mode allows all.
        if config.dynamic_peers.certificate_file.is_none() || config.dynamic_peers.private_key.is_none() {
            (
                create_server_cert_default_allow(config.dynamic_peers.hostname.unwrap())
                    .expect("unable to create self-signed server cert"),
                None,
            )
        } else {
            create_server_cert_enforce_peer(config.dynamic_peers, config.static_peers)
                .expect("unable to create tls server config")
        };
    let histogram_listener = std::net::TcpListener::bind(config.histogram_address).unwrap();
    let metrics_listener = std::net::TcpListener::bind(config.metrics_address).unwrap();
    let acceptor = TlsAcceptor::new(tls_config);
    let client = make_reqwest_client(config.remote_write, APP_USER_AGENT);
    let histogram_relay = histogram_relay::start_prometheus_server(histogram_listener);
    let registry_service = metrics::start_prometheus_server(metrics_listener);
    let prometheus_registry = registry_service.default_registry();
    prometheus_registry
        .register(mysten_metrics::uptime_metric(
            "sui-proxy",
            VERSION,
            "unavailable",
        ))
        .unwrap();
    let app = app(
        Labels {
            network: config.network,
            inventory_hostname: env::var("INVENTORY_HOSTNAME")
                .expect("INVENTORY_HOSTNAME not found in environment"),
        },
        client,
        histogram_relay,
        allower,
    );

    server(listener, app, Some(acceptor)).await.unwrap();

    Ok(())
}
