// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use multiaddr::Multiaddr;
use std::{path::PathBuf, time::Duration};
use sui_config::{Config, NodeConfig};
use tracing::info;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
struct Args {
    #[clap(long)]
    pub config_path: PathBuf,

    #[clap(long, help = "Specify address to listen on")]
    listen_address: Option<Multiaddr>,
}

// For memory profiling info see https://github.com/jemalloc/jemalloc/wiki/Use-Case%3A-Heap-Profiling
// Example: set JE_MALLOC_CONF or _RJEM_MALLOC_CONF to:
//   prof:true,lg_prof_interval:24,lg_prof_sample:19
// The above means: turn on profiling, sample every 2^19 or 512KB bytes allocated,
//   and dump out profile every 2^24 or 16MB of memory allocated.
//
// See [doc/src/contribute/observability.md] for more info.
#[cfg(not(target_env = "msvc"))]
use jemalloc_ctl::{epoch, stats};
#[cfg(not(target_env = "msvc"))]
use jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let _guard = telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
        .with_env()
        .init();

    let args = Args::parse();

    let mut config = NodeConfig::load(&args.config_path)?;

    if let Some(listen_address) = args.listen_address {
        config.network_address = listen_address;
    }

    #[cfg(not(target_env = "msvc"))]
    {
        use jemalloc_ctl::config;
        let malloc_conf = config::malloc_conf::mib().unwrap();
        info!("Default Jemalloc conf: {}", malloc_conf.read().unwrap());

        std::thread::spawn(|| {
            loop {
                // many statistics are cached and only updated when the epoch is advanced.
                epoch::advance().unwrap();

                let allocated = stats::allocated::read().unwrap() / (1024 * 1024);
                let resident = stats::resident::read().unwrap() / (1024 * 1024);
                info!(
                    "Jemalloc: {} MB allocated / {} MB resident",
                    allocated, resident
                );
                std::thread::sleep(Duration::from_secs(60));
            }
        });
    }

    let node = sui_node::SuiNode::start(&config).await?;
    node.wait().await?;

    Ok(())
}
