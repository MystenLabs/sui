// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use sui_proxy::config::ProxyConfig;
use sui_proxy::{
    admin::{
        app, create_server_cert_default_allow, create_server_cert_enforce_peer,
        make_reqwest_client, server, Labels, VERSION,
    },
    config::load,
    histogram_relay, metrics,
};
use sui_tls::TlsAcceptor;
use telemetry_subscribers::TelemetryConfig;
use tracing::info;

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
        if config.json_rpc.certificate_file.is_none() || config.json_rpc.private_key.is_none() {
            (
                create_server_cert_default_allow(config.json_rpc.hostname.unwrap())
                    .expect("unable to create self-signed server cert"),
                None,
            )
        } else {
            create_server_cert_enforce_peer(config.json_rpc)
                .expect("unable to create tls server config")
        };
    let histogram_listener = std::net::TcpListener::bind(config.histogram_address).unwrap();
    let metrics_listener = std::net::TcpListener::bind(config.metrics_address).unwrap();
    let acceptor = TlsAcceptor::new(tls_config);
    let client = make_reqwest_client(config.remote_write);
    let histogram_relay = histogram_relay::start_prometheus_server(histogram_listener);
    let registry_service = metrics::start_prometheus_server(metrics_listener);
    let prometheus_registry = registry_service.default_registry();
    prometheus_registry
        .register(mysten_metrics::uptime_metric(VERSION))
        .unwrap();
    let app = app(
        Labels {
            network: config.network,
            inventory_hostname: config.inventory_hostname,
        },
        client,
        histogram_relay,
        allower,
    );

    server(listener, app, Some(acceptor)).await.unwrap();

    Ok(())
}
