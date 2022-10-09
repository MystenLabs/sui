// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use multiaddr::Multiaddr;
use std::path::PathBuf;
use std::time::Duration;
use sui_config::{Config, NodeConfig};
use sui_node::metrics;
use sui_telemetry::send_telemetry_event;
use tokio::task;
use tokio::time::sleep;
use tracing::{info, warn};

#[derive(Parser)]
#[clap(rename_all = "kebab-case", version)]
struct Args {
    #[clap(long)]
    pub config_path: PathBuf,

    #[clap(long, help = "Specify address to listen on")]
    listen_address: Option<Multiaddr>,
}

// Memory profiling is now done automatically based on increases in total memory usage.
// Set JE_MALLOC_CONF or _RJEM_MALLOC_CONF to:  prof:true
// See [doc/src/contribute/observability.md] for more info.
// For more memory profiling info see https://github.com/jemalloc/jemalloc/wiki/Use-Case%3A-Heap-Profiling
#[cfg(not(target_env = "msvc"))]
use jemalloc_ctl::{epoch, stats};
#[cfg(not(target_env = "msvc"))]
use jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

// Ratio of memory used compared to before that triggers a new profiling dump
const MEMORY_INCREASE_PROFILING_RATIO: f64 = 1.2;
// Interval between checks for memory profile dumps
const MEMORY_PROFILING_INTERVAL_SECS: u64 = 300;
const PROF_DUMP: &[u8] = b"prof.dump\0";

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut config = NodeConfig::load(&args.config_path)?;

    let prometheus_registry = metrics::start_prometheus_server(config.metrics_address);
    info!(
        "Started Prometheus HTTP endpoint at {}",
        config.metrics_address
    );

    // Initialize logging
    let (_guard, filter_handle) =
        telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
            .with_env()
            .with_prom_registry(&prometheus_registry)
            .init();

    if let Some(listen_address) = args.listen_address {
        config.network_address = listen_address;
    }

    #[cfg(not(target_env = "msvc"))]
    {
        use jemalloc_ctl::config;
        use std::ffi::CString;
        use std::time::Duration;
        use tracing::info;

        let malloc_conf = config::malloc_conf::mib().unwrap();
        info!("Default Jemalloc conf: {}", malloc_conf.read().unwrap());

        std::thread::spawn(|| {
            // This is the initial size of memory beyond which profiles are dumped
            let mut last_allocated_mb = 100;
            loop {
                // many statistics are cached and only updated when the epoch is advanced.
                epoch::advance().unwrap();

                // NOTE: The below code does not return values when a malloc-based profiler like Bytehound
                // is used.  Bytehound does not implement the stat APIs needed.
                let allocated = stats::allocated::read().unwrap() / (1024 * 1024);
                let resident = stats::resident::read().unwrap() / (1024 * 1024);
                info!(
                    "Jemalloc: {} MB allocated / {} MB resident",
                    allocated, resident
                );

                // TODO: split this out into mysten-infra so everyone can pick it up
                // The reason why we use manual code to dump out profiles is because the automatic JEPROF
                // options dump out too often.  We really just want profiles when the retained memory
                // keeps growing, as we want to know why.
                // Setting the timestamp and memory size in the filename helps us pick the profiles
                // to use when doing analysis.  Default JEPROF profiles just have a counter which is not
                // helpful.  This helps correlate to time and total memory consumed.
                //
                // NOTE: One needs to set MALLOC_CONF to `prof:true` for the below to work
                if (allocated as f64 / last_allocated_mb as f64) > MEMORY_INCREASE_PROFILING_RATIO {
                    info!("Significant memory increase registered, dumping profile: new = {}, old = {}",
                          allocated, last_allocated_mb);
                    last_allocated_mb = allocated;

                    // Formulate profiling filename based on ISO8601 timestamp and number of MBs
                    let dt = chrono::offset::Local::now();
                    let dt_str = dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                    let dump_name = format!("jeprof.{}.{}MB.heap", dt_str, allocated);

                    // Trigger profiling dump
                    let dump_name_cstr = CString::new(dump_name).expect("Cannot create dump name");
                    unsafe {
                        if jemalloc_ctl::raw::write(PROF_DUMP, dump_name_cstr.as_ptr()).is_err() {
                            warn!("Cannot dump memory profile, is _RJEM_MALLOC_CONF set to prof:true?");
                        }
                    }
                }
                std::thread::sleep(Duration::from_secs(MEMORY_PROFILING_INTERVAL_SECS));
            }
        });
    }

    let is_validator = config.consensus_config().is_some();
    task::spawn(async move {
        loop {
            sleep(Duration::from_secs(3600)).await;
            send_telemetry_event(is_validator).await;
        }
    });

    sui_node::admin::start_admin_server(config.admin_interface_port, filter_handle);

    let node = sui_node::SuiNode::start(&config, prometheus_registry).await?;
    node.wait().await?;

    Ok(())
}
