// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::io::Write;
use std::io::{stderr, stdout};
use std::path::PathBuf;

use async_trait::async_trait;
use colored::Colorize;
use opentelemetry::global;
use opentelemetry::sdk::propagation::TraceContextPropagator;
use structopt::clap::{App, AppSettings};
use structopt::StructOpt;
use tracing::error;
use tracing::subscriber::set_global_default;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Registry};

use sui::config::{Config, WalletConfig};
use sui::shell::{
    install_shell_plugins, AsyncHandler, CacheKey, CommandStructure, CompletionCache, Shell,
};
use sui::wallet_commands::*;

const SUI: &str = "   _____       _    _       __      ____     __
  / ___/__  __(_)  | |     / /___ _/ / /__  / /_
  \\__ \\/ / / / /   | | /| / / __ `/ / / _ \\/ __/
 ___/ / /_/ / /    | |/ |/ / /_/ / / /  __/ /_
/____/\\__,_/_/     |__/|__/\\__,_/_/_/\\___/\\__/";

#[derive(StructOpt)]
#[structopt(
    name = "Sui Demo Wallet",
    about = "A Byzantine fault tolerant chain with low-latency finality and high throughput",
    rename_all = "kebab-case"
)]
struct ClientOpt {
    #[structopt(long, global = true)]
    no_shell: bool,
    /// Sets the file storing the state of our user accounts (an empty one will be created if missing)
    #[structopt(long, default_value = "./wallet.conf")]
    config: PathBuf,
    /// Subcommands. Acceptable values are transfer, query_objects, benchmark, and create_accounts.
    #[structopt(subcommand)]
    cmd: Option<WalletCommands>,
    /// Return command outputs in json format.
    #[structopt(long, global = true)]
    json: bool,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let file_appender = tracing_appender::rolling::daily("", "wallet.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let format = tracing_subscriber::fmt::layer()
        .with_thread_names(false)
        .with_writer(non_blocking)
        .compact();

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = Registry::default().with(env_filter).with(format);

    if std::env::var("SUI_TRACING_ENABLE").is_ok() {
        // Install a tracer to send traces to Jaeger.  Batching for better performance.
        let tracer = opentelemetry_jaeger::new_pipeline()
            .with_service_name("Sui wallet")
            .with_max_packet_size(9216) // Default max UDP packet size on OSX
            .with_auto_split_batch(true) // Auto split batches so they fit under packet size
            .install_batch(opentelemetry::runtime::Tokio)
            .expect("Could not create async Tracer");

        // Create a tracing subscriber with the configured tracer
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

        // Enable Trace Contexts for tying spans together
        global::set_text_map_propagator(TraceContextPropagator::new());

        set_global_default(subscriber.with(telemetry)).expect("Failed to set subscriber");
    } else {
        set_global_default(subscriber).expect("Failed to set subscriber");
    }

    let mut app: App = ClientOpt::clap();
    app = app.unset_setting(AppSettings::NoBinaryName);
    let options: ClientOpt = ClientOpt::from_clap(&app.get_matches());
    let wallet_conf_path = options.config;
    let config =
        WalletConfig::read_or_create(&wallet_conf_path).expect("Unable to read wallet config");
    let addresses = config.accounts.clone();
    let mut context = WalletContext::new(config)?;

    // Sync all accounts on start up.
    for address in addresses {
        WalletCommands::SyncClientState { address }
            .execute(&mut context)
            .await?;
    }

    let mut out = stdout();

    if !options.no_shell {
        let app: App = WalletCommands::clap();
        writeln!(out, "{}", SUI.cyan().bold())?;
        let version = app
            .p
            .meta
            .long_version
            .unwrap_or_else(|| app.p.meta.version.unwrap_or("unknown"));
        writeln!(out, "--- sui wallet {} ---", version)?;
        writeln!(out)?;
        writeln!(out, "{}", context.config)?;
        writeln!(out, "Welcome to the Sui interactive shell.")?;
        writeln!(out)?;

        let mut shell = Shell::new(
            "sui>-$ ".bold().green(),
            context,
            ClientCommandHandler,
            CommandStructure::from_clap(&install_shell_plugins(app)),
        );

        shell.run_async(&mut out, &mut stderr()).await?;
    } else if let Some(mut cmd) = options.cmd {
        cmd.execute(&mut context).await?.print(!options.json);
    }
    Ok(())
}

struct ClientCommandHandler;

#[async_trait]
impl AsyncHandler<WalletContext> for ClientCommandHandler {
    async fn handle_async(
        &self,
        args: Vec<String>,
        context: &mut WalletContext,
        completion_cache: CompletionCache,
    ) -> bool {
        if let Err(e) = handle_command(get_command(args), context, completion_cache).await {
            error!("{}", e.to_string().red());
        }
        false
    }
}

fn get_command(args: Vec<String>) -> Result<WalletOpts, anyhow::Error> {
    let app: App = install_shell_plugins(WalletOpts::clap());
    Ok(WalletOpts::from_clap(&app.get_matches_from_safe(args)?))
}

async fn handle_command(
    wallet_opts: Result<WalletOpts, anyhow::Error>,
    context: &mut WalletContext,
    completion_cache: CompletionCache,
) -> Result<(), anyhow::Error> {
    let mut wallet_opts = wallet_opts?;
    let result = wallet_opts.command.execute(context).await?;

    // Update completion cache
    // TODO: Completion data are keyed by strings, are there ways to make it more error proof?
    if let Ok(mut cache) = completion_cache.write() {
        match result {
            WalletCommandResult::Addresses(ref addresses) => {
                let addresses = addresses
                    .iter()
                    .map(|addr| format!("{}", addr))
                    .collect::<Vec<_>>();
                cache.insert(CacheKey::flag("--address"), addresses.clone());
                cache.insert(CacheKey::flag("--to"), addresses);
            }
            WalletCommandResult::Objects(ref objects) => {
                let objects = objects
                    .iter()
                    .map(|(object_id, _, _)| format!("{}", object_id))
                    .collect::<Vec<_>>();
                cache.insert(CacheKey::new("object", "--id"), objects.clone());
                cache.insert(CacheKey::flag("--gas"), objects.clone());
                cache.insert(CacheKey::flag("--object-id"), objects);
            }
            _ => {}
        }
    }
    result.print(!wallet_opts.json);
    Ok(())
}
