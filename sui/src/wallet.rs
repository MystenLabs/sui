// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use async_trait::async_trait;
use colored::Colorize;
use std::io;
use std::path::PathBuf;
use structopt::clap::{App, AppSettings};
use structopt::StructOpt;
use sui::config::WalletConfig;
use sui::shell::{AsyncHandler, CommandStructure, Shell};
use sui::utils::Config;
use sui::wallet_commands::*;

const FAST_X: &str = "   _____       _    _       __      ____     __
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
    #[structopt(long)]
    no_shell: bool,
    /// Sets the file storing the state of our user accounts (an empty one will be created if missing)
    #[structopt(long, default_value = "./wallet.conf")]
    config: PathBuf,
    /// Subcommands. Acceptable values are transfer, query_objects, benchmark, and create_accounts.
    #[structopt(subcommand)]
    cmd: Option<WalletCommands>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let format = tracing_subscriber::fmt::format()
        .with_level(false)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .without_time()
        .compact();
    tracing_subscriber::fmt().event_format(format).init();

    let mut app: App = ClientOpt::clap();
    app = app.unset_setting(AppSettings::NoBinaryName);
    let options: ClientOpt = ClientOpt::from_clap(&app.get_matches());
    let wallet_conf_path = options.config;
    let config =
        WalletConfig::read_or_create(&wallet_conf_path).expect("Unable to read wallet config");
    let addresses = config
        .accounts
        .iter()
        .map(|info| info.address)
        .collect::<Vec<_>>();
    let mut context = WalletContext::new(config)?;

    // Sync all accounts on start up.
    for address in addresses {
        WalletCommands::SyncClientState { address }
            .execute(&mut context)
            .await?;
    }

    if !options.no_shell {
        let app: App = WalletCommands::clap();
        println!("{}", FAST_X.cyan().bold());
        print!("--- Sui");
        app.write_long_version(&mut io::stdout())?;
        println!(" ---");
        println!("{}", context.config);
        println!();
        println!("Welcome to the Sui interactive shell.");
        println!();

        let mut shell = Shell {
            prompt: "sui>-$ ",
            state: context,
            handler: ClientCommandHandler,
            description: String::new(),
            command: CommandStructure::from_clap(&app),
        };
        shell.run_async().await?;
    } else if let Some(mut cmd) = options.cmd {
        cmd.execute(&mut context).await?;
    }
    Ok(())
}

struct ClientCommandHandler;

#[async_trait]
impl AsyncHandler<WalletContext> for ClientCommandHandler {
    async fn handle_async(&self, args: Vec<String>, context: &mut WalletContext, _: &str) -> bool {
        let command: Result<WalletCommands, _> = WalletCommands::from_iter_safe(args);
        match command {
            Ok(mut cmd) => {
                if let Err(e) = cmd.execute(context).await {
                    eprintln!("{}", format!("{}", e).red());
                }
            }
            Err(e) => {
                eprintln!("{}", e.message.red());
            }
        }
        false
    }
}
