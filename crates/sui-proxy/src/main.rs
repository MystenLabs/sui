// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use sui_proxy::config::ProxyConfig;
use sui_proxy::{
    admin::{
        app, create_server_cert_default_allow, create_server_cert_enforce_peer,
        make_reqwest_client, server,
    },
    config::load,
    metrics,
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
    let acceptor = TlsAcceptor::new(tls_config);
    let client = make_reqwest_client(config.remote_write);
    let app = app(config.network, client, allower);

    let registry_service = metrics::start_prometheus_server(config.metrics_address);
    let prometheus_registry = registry_service.default_registry();
    prometheus_registry
        .register(mysten_metrics::uptime_metric(VERSION))
        .unwrap();

    server(listener, app, Some(acceptor)).await.unwrap();

    Ok(())
}

const GIT_REVISION: &str = {
    if let Some(revision) = option_env!("GIT_REVISION") {
        revision
    } else {
        git_version::git_version!(
            args = ["--always", "--dirty", "--exclude", "*"],
            fallback = "DIRTY"
        )
    }
};
const VERSION: &str = const_str::concat!(env!("CARGO_PKG_VERSION"), "-", GIT_REVISION);
