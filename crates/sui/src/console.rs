// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::client_commands::SwitchResponse;
use crate::client_commands::{SuiClientCommandResult, SuiClientCommands, WalletContext};
use crate::shell::{
    install_shell_plugins, AsyncHandler, CacheKey, CommandStructure, CompletionCache, Shell,
};
use async_trait::async_trait;
use clap::Command;
use clap::CommandFactory;
use clap::FromArgMatches;
use clap::Parser;
use colored::Colorize;
use std::io::{stderr, Write};
use std::ops::Deref;
const SUI: &str = "   _____       _    ______                       __   
  / ___/__  __(_)  / ____/___  ____  _________  / /__ 
  \\__ \\/ / / / /  / /   / __ \\/ __ \\/ ___/ __ \\/ / _ \\
 ___/ / /_/ / /  / /___/ /_/ / / / (__  ) /_/ / /  __/
/____/\\__,_/_/   \\____/\\____/_/ /_/____/\\____/_/\\___/";

#[derive(Parser)]
#[clap(name = "", rename_all = "kebab-case", no_binary_name = true)]
pub struct ConsoleOpts {
    #[clap(subcommand)]
    pub command: SuiClientCommands,
    /// Returns command outputs in JSON format.
    #[clap(long, global = true)]
    pub json: bool,
}

pub async fn start_console(
    context: WalletContext,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), anyhow::Error> {
    let app: Command = SuiClientCommands::command();
    writeln!(out, "{}", SUI.cyan().bold())?;
    let mut version = app
        .get_long_version()
        .unwrap_or_else(|| app.get_version().unwrap_or("unknown"))
        .to_owned();
    if let Some(git_rev) = std::option_env!("GIT_REVISION") {
        version.push('-');
        version.push_str(git_rev);
    }
    writeln!(out, "--- Sui Console {version} ---")?;
    writeln!(out)?;
    writeln!(out, "{}", context.config.deref())?;
    writeln!(out, "Welcome to the Sui interactive console.")?;
    writeln!(out)?;

    let mut shell = Shell::new(
        "sui>-$ ".bold().green(),
        context,
        ClientCommandHandler,
        CommandStructure::from_clap(&install_shell_plugins(app)),
    );

    shell.run_async(out, err).await
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
        match handle_command(get_command(args), context, completion_cache).await {
            Err(e) => {
                let _err = writeln!(stderr(), "{}", e.to_string().red());
                false
            }
            Ok(return_value) => return_value,
        }
    }
}

fn get_command(args: Vec<String>) -> Result<ConsoleOpts, anyhow::Error> {
    let app: Command = install_shell_plugins(ConsoleOpts::command());
    Ok(ConsoleOpts::from_arg_matches(
        &app.try_get_matches_from(args)?,
    )?)
}

async fn handle_command(
    wallet_opts: Result<ConsoleOpts, anyhow::Error>,
    context: &mut WalletContext,
    completion_cache: CompletionCache,
) -> Result<bool, anyhow::Error> {
    let wallet_opts = wallet_opts?;
    let result = wallet_opts.command.execute(context).await?;

    // Update completion cache
    // TODO: Completion data are keyed by strings, are there ways to make it more error proof?
    if let Ok(mut cache) = completion_cache.write() {
        match result {
            SuiClientCommandResult::Addresses(ref addresses) => {
                let addresses = addresses
                    .iter()
                    .map(|addr| format!("{addr}"))
                    .collect::<Vec<_>>();
                cache.insert(CacheKey::flag("--address"), addresses.clone());
                cache.insert(CacheKey::flag("--to"), addresses);
            }
            SuiClientCommandResult::Objects(ref objects) => {
                let objects = objects
                    .iter()
                    .map(|oref| format!("{}", oref.object_id))
                    .collect::<Vec<_>>();
                cache.insert(CacheKey::new("object", "--id"), objects.clone());
                cache.insert(CacheKey::flag("--gas"), objects.clone());
                cache.insert(CacheKey::flag("--coin-object-id"), objects);
            }
            _ => {}
        }
    }
    result.print(!wallet_opts.json);

    // Quit shell after gateway switch
    if matches!(
        result,
        SuiClientCommandResult::Switch(SwitchResponse {
            gateway: Some(_),
            ..
        })
    ) {
        println!("Gateway switch completed, please restart Sui console.");
        return Ok(true);
    }
    Ok(false)
}
