// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::ptb_builder::{
    ast::{ParsedProgram, Program},
    errors::{FileTable, PTBError},
};
use crate::client_ptb::{
    displays::Pretty,
    ptb_builder::{build_ptb::PTBBuilder, errors::render_errors, parser::ProgramParser},
};

use anyhow::{anyhow, Error};
use clap::{arg, Args};
use move_core_types::account_address::AccountAddress;
use serde::Serialize;
use shared_crypto::intent::Intent;
use std::collections::{BTreeMap, BTreeSet};
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponseOptions,
};
use sui_keys::keystore::AccountKeystore;
use sui_sdk::{wallet_context::WalletContext, SuiClient};
use sui_types::{
    digests::TransactionDigest,
    gas::GasCostSummary,
    quorum_driver_types::ExecuteTransactionRequestType,
    transaction::{ProgrammableTransaction, Transaction, TransactionData},
};

#[derive(Clone, Debug, Args)]
pub struct PTB {
    #[clap(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

pub struct PTBPreview<'a> {
    pub program: &'a Program,
}

#[derive(Serialize)]
pub struct Summary {
    pub digest: TransactionDigest,
    pub status: SuiExecutionStatus,
    pub gas_cost: GasCostSummary,
}

impl PTB {
    /// Parses and executes the PTB with the sender as the current active address
    pub async fn execute(
        self,
        args: Vec<String>,
        context: &mut WalletContext,
    ) -> Result<(), Error> {
        let arg_string = args.join(" ");
        let mut file_table = BTreeMap::new();
        let (program, program_metadata) = match Self::parse_ptb_commands(arg_string, &mut file_table)
        {
            Err(errors) => {
                let suffix = if errors.len() > 1 { "s" } else { "" };
                let rendered = render_errors(&file_table, errors);
                eprintln!("Encountered error{suffix} when parsing PTB:");
                for e in rendered.iter() {
                    eprintln!("{:?}", e);
                }
                anyhow::bail!("Could not build PTB due to previous error{suffix}");
            }
            Ok(parsed) => parsed,
        };

        if program_metadata.preview_set {
            println!("{}", PTBPreview { program: &program });
            return Ok(());
        }

        let client = context.get_client().await?;

        let (ptb, budget) = match Self::build_ptb(program, context, client).await {
            Err(errors) => {
                let suffix = if errors.len() > 1 { "s" } else { "" };
                eprintln!("Encountered error{suffix} when building PTB:");
                let rendered = render_errors(&file_table, errors);
                for e in rendered.iter() {
                    eprintln!("{:?}", e);
                }
                anyhow::bail!("Could not build PTB due to previous error{suffix}");
            }
            Ok(x) => x,
        };

        // get all the metadata needed for executing the PTB: sender, gas, signing tx
        // get sender's address -- active address
        let Some(sender) = context.config.active_address else {
            anyhow::bail!("No active address, cannot execute PTB");
        };

        // find the gas coins if we have no gas coin given
        let coins = if let Some(gas) = program_metadata.gas_object_id {
            context.get_object_ref(gas.value).await?
        } else {
            context
                .gas_for_owner_budget(sender, budget, BTreeSet::new())
                .await?
                .1
                .object_ref()
        };

        // get the gas price
        let gas_price = context
            .get_client()
            .await?
            .read_api()
            .get_reference_gas_price()
            .await?;
        // create the transaction data that will be sent to the network
        let tx_data =
            TransactionData::new_programmable(sender, vec![coins], ptb, budget, gas_price);
        // sign the tx
        let signature =
            context
                .config
                .keystore
                .sign_secure(&sender, &tx_data, Intent::sui_transaction())?;

        // execute the transaction
        let transaction_response = context
            .get_client()
            .await?
            .quorum_driver_api()
            .execute_transaction_block(
                Transaction::from_data(tx_data, vec![signature]),
                SuiTransactionBlockResponseOptions::full_content(),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?;

        if let Some(effects) = transaction_response.effects.as_ref() {
            if effects.status().is_err() {
                return Err(anyhow!(
                    "PTB execution {}. Transaction digest is: {}",
                    Pretty(effects.status()),
                    effects.transaction_digest()
                ));
            }
        }

        let summary = {
            let effects = transaction_response.effects.as_ref().ok_or_else(|| {
                anyhow!("Internal error: no transaction effects after PTB was executed.")
            })?;
            Summary {
                digest: transaction_response.digest,
                status: effects.status().clone(),
                gas_cost: effects.gas_cost_summary().clone(),
            }
        };

        if program_metadata.json_set {
            let json_string = if program_metadata.summary_set {
                serde_json::to_string_pretty(&serde_json::json!(summary))
                    .map_err(|_| anyhow!("Cannot serialize PTB result to json"))?
            } else {
                serde_json::to_string_pretty(&serde_json::json!(transaction_response))
                    .map_err(|_| anyhow!("Cannot serialize PTB result to json"))?
            };
            println!("{}", json_string);
        } else if program_metadata.summary_set {
            println!("{}", Pretty(&summary));
        } else {
            println!("{}", transaction_response);
        }

        Ok(())
    }

    /// Exposed for testing
    pub async fn build_ptb(
        program: Program,
        context: &WalletContext,
        client: SuiClient,
    ) -> Result<(ProgrammableTransaction, u64), Vec<PTBError>> {
        let starting_addresses = context
            .config
            .keystore
            .addresses_with_alias()
            .into_iter()
            .map(|(sa, alias)| (alias.alias.clone(), AccountAddress::from(*sa)))
            .collect();
        let builder = PTBBuilder::new(starting_addresses, client.read_api());
        builder.build(program).await
    }

    /// Exposed for testing
    pub fn parse_ptb_commands(
        arg_string: String,
        file_table: &mut FileTable,
    ) -> Result<ParsedProgram, Vec<PTBError>> {
        ProgramParser::new(arg_string, file_table)
            .map_err(|e| vec![e])?
            .parse()
    }
}

fn _ptb_description() -> clap::Command {
    clap::Command::new("ptb")
        .about("Build, preview, and execute programmable transaction blocks.")
        .arg(arg!(
            --file <FILE>
                "Path to a file containing transactions to include in this PTB."
        ))
        .arg(arg!(
            --assign <ASSIGN>...
                "Assign a value to use later in the PTB. If only a name is supplied, the result of \
                 the last transaction is binded to that name. If a name and value are \
                 supplied, then the name is binded to that value."
        ))
        .arg(arg!(
            --gas <ID> ...
                "The object ID of the gas coin to use."
        ))
        .arg(arg!(
            --"gas-budget" <MIST>
                "The gas budget for the transaction, in MIST."
        ))
        .arg(arg!(
            --"make-move-vec" <MAKE_MOVE_VEC>
            r#"Given n-values of the same type, it constructs a vector. For non objects or an empty vector, the type tag must be specified: --make-move-vec "<u64>" "[1]" "#
        ))
    //     #[clap(long, num_args(1..))]
    //     make_move_vec: Vec<String>,
    //     /// Merge N coins into the provided coin: --merge-coins into_coin "[coin1,coin2,coin3]"
    //     #[clap(long, num_args(1..))]
    //     merge_coins: Vec<String>,
    //     /// Make a move call to a function
    //     #[clap(long, num_args(1..))]
    //     move_call: Vec<String>,
    //     /// Split the coin into N coins as per the given amount.
    //     /// On zsh, the vector needs to be given in quotes: --split-coins coin_to_split "[amount1,amount2]"
    //     #[clap(long, num_args(1..))]
    //     split_coins: Vec<String>,
    //     /// Transfer objects to the address. E.g., --transfer-objects to_address "[obj1, obj2]"
    //     #[clap(long, num_args(1..))]
    //     transfer_objects: Vec<String>,
    //     /// Publish the move package. It takes as input the folder where the package exists.
    //     #[clap(long, num_args(1..))]
    //     publish: Vec<String>,
    //     /// Upgrade the move package. It takes as input the folder where the package exists.
    //     #[clap(long, num_args(1..))]
    //     upgrade: Vec<String>,
    //     /// Preview the PTB instead of executing it
    //     #[clap(long)]
    //     preview: bool,
    //     /// Enable shadown warning when including other PTB files.
    //     /// Off by default.
    //     #[clap(long)]
    //     warn_shadows: bool,
    //     /// Pick gas budget strategy if multiple gas-budgets are provided.
    //     #[clap(long)]
    //     pick_gas_budget: Option<PTBGas>,
    //     /// Show only a short summary (digest, execution status, gas cost).
    //     /// Do not use this flag when you need all the transaction data and the execution effects.
    //     #[clap(long)]
    //     summary: bool,
}
