// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::Result;
use clap::{Parser, Subcommand};
use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{StructTag, TypeTag};
use sui_config::{sui_config_dir, SUI_CLIENT_CONFIG};
use sui_sdk::types::base_types::{ObjectID, SuiAddress};
use sui_sdk::wallet_context::WalletContext;
use tracing::debug;

use rust_client::tx_run;
use rust_client::tx_run::{AppCommand, AppConfig};

/// Regulated coin command line interface
#[derive(Parser, Debug)]
#[command(name = "rust-client")]
struct Cli {
    /// The address of the contract the coin is issued.
    /// If none is passed, environment variable `PACKAGE_ID` will be used.
    #[arg(long = "package-id", short = 'p')]
    package_id: Option<String>,
    /// The module that issues the coin.
    /// If none is passed, environment variable `MODULE_NAME` will be used.
    /// Lastly defaults to "regulated_coin".
    #[arg(long = "module", short = 'm')]
    module: Option<String>,
    #[clap(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    /// Add an address to allow-list
    #[command(name = "deny-list-add")]
    DenyListAdd {
        /// The address to insert to deny-list
        #[arg(value_parser)]
        address: String,
    },
    /// Remove an address from deny-list
    #[clap(name = "deny-list-remove")]
    DenyListRemove {
        /// The address to remove from deny-list
        #[arg(value_parser)]
        address: String,
    },
    /// Mint and transfer coin
    MintAndTransfer {
        /// Balance of the new Coin
        #[arg(long = "balance", short = 'b')]
        balance: u64,
        /// The address to transfer the new Coin
        #[arg(value_parser)]
        address: String,
    },
    /// Transfer coin from the sui client's active address
    Transfer {
        /// The Coin to transfer
        #[arg(long = "coin", short = 'c')]
        coin: String,
        /// The address to transfer the Coin
        #[arg(value_parser)]
        address: String,
    },
    /// Burn coin inside the sui client's active address
    Burn {
        /// The Coin to burn
        #[arg(value_parser)]
        coin: String,
    },
}

async fn cli_parse() -> Result<(AppConfig, AppCommand)> {
    let Cli {
        package_id,
        module,
        command,
    } = Cli::parse();
    let package_id_str = match package_id {
        Some(package_id) => package_id,
        None => {
            dotenvy::dotenv().ok();
            std::env::var("PACKAGE_ID")?
        }
    };
    let package_id = ObjectID::from_hex_literal(&package_id_str)?;
    let module = match module {
        Some(module) => module,
        None => {
            dotenvy::dotenv().ok();
            match std::env::var("MODULE_NAME") {
                Ok(module) => module,
                Err(_) => "regulated_coin".to_string()
            }
        }
    };
    let otw = module.to_uppercase();
    let type_tag = TypeTag::Struct(Box::new(StructTag {
        address: AccountAddress::new(package_id.as_ref().try_into()?),
        module: Identifier::from_str(&module)?,
        name: Identifier::from_str(&otw)?,
        type_params: vec![],
    }));
    let wallet_context =
        WalletContext::new(&sui_config_dir()?.join(SUI_CLIENT_CONFIG), None, None).await?;

    let command = match command {
        CliCommand::DenyListAdd { address } => {
            AppCommand::DenyListAdd(SuiAddress::from_str(&address)?)
        }
        CliCommand::DenyListRemove { address } => {
            AppCommand::DenyListRemove(SuiAddress::from_str(&address)?)
        }
        CliCommand::MintAndTransfer { balance, address } => {
            AppCommand::MintAndTransfer(balance, SuiAddress::from_str(&address)?)
        }
        CliCommand::Transfer { coin, address } => AppCommand::Transfer(
            ObjectID::from_hex_literal(&coin)?,
            SuiAddress::from_str(&address)?,
        ),
        CliCommand::Burn { coin } => AppCommand::Burn(ObjectID::from_hex_literal(&coin)?),
    };

    let client = wallet_context.get_client().await?;
    Ok((
        AppConfig {
            client,
            wallet_context,
            type_tag,
        },
        command,
    ))
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let (config, command) = cli_parse().await?;
    let resp = tx_run::execute_command(command, config).await?;

    debug!("{:?}", resp);

    Ok(())
}
