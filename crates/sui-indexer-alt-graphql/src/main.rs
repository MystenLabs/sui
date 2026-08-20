// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use clap::Parser;
use prometheus::Registry;
use sui_futures::service::Error;
use sui_indexer_alt_graphql::args::Args;
use sui_indexer_alt_graphql::args::Command;
use sui_indexer_alt_graphql::config::IndexerConfig;
use sui_indexer_alt_graphql::config::RpcLayer;
use sui_indexer_alt_graphql::start_rpc;
use sui_indexer_alt_metrics::MetricsService;
use sui_indexer_alt_metrics::uptime;
use telemetry_subscribers::TelemetryConfig;
use tokio::fs;

// Define the `GIT_REVISION` const
bin_version::git_revision!();

static VERSION: &str = const_str::concat!(
    env!("CARGO_PKG_VERSION_MAJOR"),
    ".",
    env!("CARGO_PKG_VERSION_MINOR"),
    ".",
    env!("CARGO_PKG_VERSION_PATCH"),
    "-",
    GIT_REVISION
);

#[cfg(all(not(target_env = "msvc"), feature = "jemalloc"))]
#[global_allocator]
static JEMALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// Logs jemalloc's profiling state (`opt.prof`/`opt.prof_active`/`opt.prof_gdump`) at startup,
/// so a misconfigured or silently-ignored `_RJEM_MALLOC_CONF` is visible in the logs instead of
/// manifesting only as "no heap profiles ever show up". If profiling is active, also spawns a
/// background task that writes a heap profile via `prof.dump` every 5 minutes: jemalloc's own
/// `prof_gdump` (dump on every new high-water mark) is enough to *start* profiling with, but a
/// fixed cadence is what actually gives an operator a profile to inspect on a predictable
/// timeline, and additionally captures the post-peak retention/decay behavior this allocator
/// switch is meant to fix — which an only-on-new-peak dump would tend to miss.
#[cfg(all(not(target_env = "msvc"), feature = "jemalloc"))]
fn init_jemalloc_profiling() {
    use tikv_jemalloc_ctl::raw;

    // SAFETY: these mallctl names are `\0`-terminated `bool`-typed read-only jemalloc options;
    // `raw::read` matches their C type to the requested Rust type at the FFI boundary.
    let opt_prof: bool = unsafe { raw::read(b"opt.prof\0") }.unwrap_or(false);
    let opt_prof_active: bool = unsafe { raw::read(b"opt.prof_active\0") }.unwrap_or(false);
    let opt_prof_gdump: bool = unsafe { raw::read(b"opt.prof_gdump\0") }.unwrap_or(false);

    tracing::info!(
        opt_prof,
        opt_prof_active,
        opt_prof_gdump,
        "jemalloc heap profiling status (enable with _RJEM_MALLOC_CONF=prof:true)",
    );

    if !opt_prof {
        // DIAGNOSTIC (temporary): opt.prof is a read-only reflection of whether `prof:true` was
        // honored at boot, but --enable-prof was confirmed compiled in. Try to force profiling on
        // via the read-write `prof.active` mallctl anyway, and immediately attempt a calibration
        // dump, to determine empirically whether jemalloc's profiling infrastructure was actually
        // booted (in which case this works) or not (in which case both calls fail/no-op).
        let activate: Result<(), _> = unsafe { raw::write::<bool>(b"prof.active\0", true) };
        let calibration_dump: Option<Result<(), _>> = if activate.is_ok() {
            let dump_path: *const std::os::raw::c_char = std::ptr::null();
            Some(unsafe { raw::write::<*const std::os::raw::c_char>(b"prof.dump\0", dump_path) })
        } else {
            None
        };
        tracing::warn!(
            ?activate,
            ?calibration_dump,
            "opt.prof is false; attempted to force-enable via prof.active as a diagnostic",
        );
        if !matches!(calibration_dump, Some(Ok(()))) {
            return;
        }
    }

    tokio::spawn(async {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5 * 60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // First tick fires immediately; skip it so we don't dump before there's anything
        // interesting allocated yet.
        interval.tick().await;
        loop {
            interval.tick().await;
            // A NULL path makes jemalloc dump under its default `<prof_prefix>.<pid>.<seq>.heap`
            // naming scheme (prof_prefix defaults to "jeprof", relative to the process's CWD).
            //
            // SAFETY: `prof.dump` is a write-only mallctl that takes an optional
            // `const char *` path; passing a null pointer of the correct FFI type is documented,
            // valid usage (see jemalloc(3), "prof.dump").
            let dump_path: *const std::os::raw::c_char = std::ptr::null();
            match unsafe { raw::write::<*const std::os::raw::c_char>(b"prof.dump\0", dump_path) } {
                Ok(()) => tracing::info!("wrote periodic jemalloc heap profile"),
                Err(error) => {
                    tracing::warn!(%error, "failed to write periodic jemalloc heap profile")
                }
            }
        }
    });
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Enable tracing, configured by environment variables.
    let _guard = TelemetryConfig::new()
        // ErrorLayer is disabled by default in TelemetryConfig, but enabled by default in GraphQL
        // to give useful error output for debugging request timeouts.
        .with_enable_error_layer(true)
        .with_env()
        .init();

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install CryptoProvider");

    match args.command {
        Command::Rpc {
            database_url,
            fullnode_args,
            db_args,
            kv_args,
            consistent_reader_args,
            rpc_args,
            system_package_task_args,
            metrics_args,
            config,
            indexer_config,
            subscription_args,
        } => {
            #[cfg(all(not(target_env = "msvc"), feature = "jemalloc"))]
            init_jemalloc_profiling();

            let rpc_config = if let Some(path) = config {
                let contents = fs::read_to_string(path)
                    .await
                    .context("Failed to read configuration TOML file")?;

                toml::from_str(&contents).context("Failed to parse configuration TOML file")?
            } else {
                RpcLayer::default()
            }
            .finish();

            let mut pg_pipelines = vec![];
            for path in indexer_config {
                let contents = fs::read_to_string(&path).await.with_context(|| {
                    format!(
                        "Failed to read indexer configuration TOML file: {}",
                        path.display()
                    )
                })?;

                let config: IndexerConfig = toml::from_str(&contents)
                    .context("Failed to parse indexer configuration TOML file")?;

                pg_pipelines.extend(config.pipelines().map(|p| p.to_owned()));
            }

            let registry = Registry::new_custom(Some("graphql_alt".into()), None)
                .context("Failed to create Prometheus registry.")?;

            let metrics = MetricsService::new(metrics_args, registry);

            metrics
                .registry()
                .register(uptime(VERSION)?)
                .context("Failed to register uptime metric.")?;

            let s_rpc = start_rpc(
                Some(database_url),
                fullnode_args,
                db_args,
                kv_args,
                consistent_reader_args,
                rpc_args,
                system_package_task_args,
                subscription_args,
                VERSION,
                rpc_config,
                pg_pipelines,
                metrics.registry(),
            )
            .await?;

            let s_metrics = metrics.run().await?;

            match s_rpc.attach(s_metrics).main().await {
                Ok(()) | Err(Error::Terminated) => {}

                Err(Error::Aborted) => {
                    std::process::exit(1);
                }

                Err(Error::Task(_)) => {
                    std::process::exit(2);
                }
            }
        }

        Command::GenerateConfig => {
            let config = RpcLayer::example();
            let config_toml = toml::to_string_pretty(&config)
                .context("Failed to serialize default configuration to TOML.")?;

            println!("{config_toml}");
        }
    }

    Ok(())
}
