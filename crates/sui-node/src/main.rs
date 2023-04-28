// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use mysten_common::sync::async_once_cell::AsyncOnceCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_config::{Config, NodeConfig};
use sui_core::runtime::SuiRuntimes;
use sui_node::metrics;
use sui_protocol_config::SupportedProtocolVersions;
use sui_telemetry::send_telemetry_event;
use sui_types::multiaddr::Multiaddr;
use tokio::time::sleep;
use tracing::{error, info};

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

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
#[clap(name = env!("CARGO_BIN_NAME"))]
#[clap(version = VERSION)]
struct Args {
    #[clap(long)]
    pub config_path: PathBuf,

    #[clap(long, help = "Specify address to listen on")]
    listen_address: Option<Multiaddr>,
}

fn main() {
    // Ensure that a validator never calls get_for_min_version/get_for_max_version.
    // TODO: re-enable after we figure out how to eliminate crashes in prod because of this.
    // ProtocolConfig::poison_get_for_min_version();

    let args = Args::parse();
    let mut config = NodeConfig::load(&args.config_path).unwrap();
    assert!(
        config.supported_protocol_versions.is_none(),
        "supported_protocol_versions cannot be read from the config file"
    );
    config.supported_protocol_versions = Some(SupportedProtocolVersions::SYSTEM_DEFAULT);

    let runtimes = SuiRuntimes::new(&config);
    let registry_service = {
        let _enter = runtimes.metrics.enter();
        metrics::start_prometheus_server(config.metrics_address)
    };
    let prometheus_registry = registry_service.default_registry();
    prometheus_registry
        .register(mysten_metrics::uptime_metric(VERSION))
        .unwrap();

    // Initialize logging
    let (_guard, filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        // Set a default
        .with_sample_nth(10)
        .with_target_prefix("sui_json_rpc")
        .with_env()
        .with_prom_registry(&prometheus_registry)
        .init();

    info!("Sui Node version: {VERSION}");
    info!(
        "Supported protocol versions: {:?}",
        config.supported_protocol_versions
    );

    info!(
        "Started Prometheus HTTP endpoint at {}",
        config.metrics_address
    );

    {
        let _enter = runtimes.metrics.enter();
        metrics::start_metrics_push_task(&config, registry_service.clone());
    }

    if let Some(listen_address) = args.listen_address {
        config.network_address = listen_address;
    }

    let is_validator = config.consensus_config().is_some();

    let admin_interface_port = config.admin_interface_port;

    // Run node in a separate runtime so that admin/monitoring functions continue to work
    // if it deadlocks.
    let node_one_cell = Arc::new(AsyncOnceCell::<Arc<sui_node::SuiNode>>::new());
    let node_one_cell_clone = node_one_cell.clone();
    let rpc_runtime = runtimes.json_rpc.handle().clone();

    runtimes.sui_node.spawn(async move {
        if let Err(e) = sui_node::SuiNode::start_async(
            &config,
            registry_service,
            node_one_cell_clone,
            Some(rpc_runtime),
        )
        .await
        {
            error!("Failed to start node: {e:?}");
            std::process::exit(1)
        }
        // TODO: Do we want to provide a way for the node to gracefully shutdown?
        loop {
            tokio::time::sleep(Duration::from_secs(1000)).await;
        }
    });

    let node_one_cell_clone = node_one_cell.clone();
    runtimes.metrics.spawn(async move {
        let node = node_one_cell_clone.get().await;
        sui_node::admin::run_admin_server(node, admin_interface_port, filter_handle).await
    });

    runtimes.metrics.spawn(async move {
        let node = node_one_cell.get().await;
        let state = node.state();
        loop {
            send_telemetry_event(state.clone(), is_validator).await;
            sleep(Duration::from_secs(3600)).await;
        }
    });

    // wait for SIGINT on the main thread
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(wait_termination());

    // Drop and wait all runtimes on main thread
    drop(runtimes);
}

#[cfg(not(unix))]
// On windows we wait for whatever "ctrl_c" means there
async fn wait_termination() {
    tokio::signal::ctrl_c().await.unwrap()
}

#[cfg(unix)]
// On unix we wait for both SIGINT (when run in terminal) and SIGTERM(when run in docker or other supervisor)
// Docker stop sends SIGTERM: https://www.baeldung.com/ops/docker-stop-vs-kill#:~:text=The%20docker%20stop%20commands%20issue,rather%20than%20killing%20it%20immediately.
// Systemd by default sends SIGTERM as well: https://www.freedesktop.org/software/systemd/man/systemd.kill.html
// Upstart also sends SIGTERM by default: https://upstart.ubuntu.com/cookbook/#kill-signal
async fn wait_termination() {
    use futures::future::select;
    use futures::FutureExt;
    use tokio::signal::unix::*;

    let sigint = tokio::signal::ctrl_c().map(Result::ok).boxed();
    let mut sigterm = signal(SignalKind::terminate()).unwrap();
    let sigterm_recv = sigterm.recv().boxed();
    select(sigint, sigterm_recv).await;
}
