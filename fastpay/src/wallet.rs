// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
use async_trait::async_trait;
use colored::Colorize;
use fastpay::config::*;
use fastpay::wallet_commands::*;
use shellfish::{AsyncHandler, Command, Shell};
use std::collections::HashMap;
use std::io;
use structopt::clap::{App, AppSettings};
use structopt::StructOpt;
use tracing::subscriber::set_global_default;
use tracing_subscriber::EnvFilter;

const FAST_X: &str = "    ______           __ _  __
   / ____/___ ______/ /| |/ /
  / /_  / __ `/ ___/ __/   / 
 / __/ / /_/ (__  ) /_/   |  
/_/    \\__,_/____/\\__/_/|_|        ";

#[derive(StructOpt)]
#[structopt(
    name = "FastX",
    about = "A Byzantine fault tolerant payments chain with low-latency finality and high throughput",
    rename_all = "kebab-case"
)]
struct ClientOpt {
    #[structopt(long)]
    no_shell: bool,
    /// Sets the file storing the state of our user accounts (an empty one will be created if missing)
    #[structopt(long, default_value = "./wallet.conf")]
    config: String,
    /// Subcommands. Acceptable values are transfer, query_objects, benchmark, and create_accounts.
    #[structopt(subcommand)]
    cmd: Option<ClientCommands>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber_builder =
        tracing_subscriber::fmt::Subscriber::builder().with_env_filter(env_filter);
    let subscriber = subscriber_builder.with_writer(std::io::stderr).finish();
    set_global_default(subscriber).expect("Failed to set subscriber");

    let mut app: App = ClientOpt::clap();
    app = app.unset_setting(AppSettings::NoBinaryName);
    let options: ClientOpt = ClientOpt::from_clap(&app.get_matches());
    let wallet_conf_path = options.config;
    let config =
        WalletConfig::read_or_create(&wallet_conf_path).expect("Unable to read user accounts");
    let mut context = WalletContext::new(config);

    if !options.no_shell {
        let app: App = ClientCommands::clap();
        println!("{}", FAST_X.cyan().bold());
        print!("--- FastX");
        app.write_long_version(&mut io::stdout())?;
        println!(" ---");
        println!("{}", context.config);
        println!();
        println!("Welcome to the FastX interactive shell.");
        println!();

        app.settings(&[AppSettings::NoBinaryName]);

        let mut shell = Shell {
            prompt: "fastx>-$ ",
            commands: HashMap::new(),
            state: context,
            handler: ClientCommandHandler(),
            description: String::new(),
        };
        shell.run_async().await?;
    } else if let Some(mut cmd) = options.cmd {
        cmd.execute(&mut context).await?;
    }
    Ok(())
}

struct ClientCommandHandler();

#[async_trait]
impl AsyncHandler<WalletContext> for ClientCommandHandler {
    async fn handle_async(
        &self,
        args: Vec<String>,
        _: &HashMap<&str, Command<WalletContext>>,
        context: &mut WalletContext,
        _: &str,
    ) -> bool {
        if let Some(arg) = args.get(0) {
            if let "quit" | "exit" = arg.as_str() {
                println!("Bye!");
                return true;
            }
        };

        let command: Result<ClientCommands, structopt::clap::Error> =
            ClientCommands::from_iter_safe(args);

        match command {
            Ok(mut cmd) => cmd.execute(context).await.unwrap(),
            Err(e) => {
                println!("{}", e.message);
            }
        }
        false
    }
}
