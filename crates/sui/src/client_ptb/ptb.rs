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
use clap::{arg, Args, ValueHint};
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
#[clap(disable_help_flag = true)]
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
        let (program, program_metadata) =
            match Self::parse_ptb_commands(arg_string, &mut file_table) {
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

pub fn ptb_description() -> clap::Command {
    clap::Command::new("sui client ptb")
        .about(
            "Build, preview, and execute programmable transaction blocks. Depending on your \
            shell you might have to use quotes around arrays or other passed values. \
            Use --help to see examples for how to use the PTB core commands")
        .arg(arg!(
                --"assign" <ASSIGN>
                "Assign a value to a variable name to use later in the PTB."
        )
        .long_help(
            "Assign a value to a variable name to use later in the PTB.\
            If only a name is supplied, the result of \
            the last transaction is binded to that name. If a name and value are \
            supplied, then the name is binded to that value.\n\n\
            Examples:\n --assign MYVAR 100\n --assign X [100,5000]\n --split-coins gas \
            [1000, 5000, 75000]\
            \n --assign new_coins # bind new_coins to the result of previous transaction"
        )
        .value_names(["NAME", "VALUE"]))
        .arg(
            arg!(
                --file <FILE>
                "Path to a file containing transactions to include in this PTB."
            ).value_hint(ValueHint::FilePath))
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
            "Given n-values of the same type, it constructs a vector. For non objects or an empty \
            vector, the type tag must be specified."
        )
        .long_help(
            "Given n-values of the same type, it constructs a vector. \
            For non objects or an empty vector, the type tag must be specified.\
            \n\nExamples:\
            \n --make-move-vec <u64> []\
            \n --make-move-vec <u64> [1, 2, 3, 4]\
            \n --make-move-vec <std::option::Option<u64>> [none,none]\
            \n --make-move-vec <sui::coin::Coin<sui::sui::SUI>> [gas]"
        )
        .value_names(["TYPE", "[VALUES]"]))
        .arg(arg!(
            --"merge-coins" <MERGE_COINS>
            "Merge N coins into the provided coin."
        ).long_help(
            "Merge N coins into the provided coin.\
            \n\nExamples:\
            \n --merge-coins @coin_object_id [@coin_obj_id1, @coin_obj_id2]"
            )
        .value_names(["INTO_COIN", "[COIN OBJECTS]"]))
        .arg(arg!(
            --"move-call" <MOVE_CALL>
            "Make a move call to a function."
        )
        .long_help(
            "Make a move call to a function.\n\nExamples:\
            \n --move-call 0x1::option::is_none <u64> none\
            \n --assign a none\
            \n --move-call 0x1::option::is_none <u64> a"
        )
        .value_names(["PACKAGE::MODULE::FUNCTION", "TYPE", "FUNCTION_ARGS"]))
        .arg(arg!(
            --"split-coins" <SPLIT_COINS>
            "Split the coin into N coins as per the given array of amounts."
        )
        .long_help(
            "Split the coin into N coins as per the given array of amounts.\n\nExamples:\
            \n --split-coins gas [1000, 5000, 75000]\
            \n --assign new_coins # binds the result of split-coins command to variable new_coins\
            \n --split-coins @coin_object_id [100]"
        )
        .value_names(["COIN", "[AMMOUNT]"]))
        .arg(arg!(
            --"transfer-objects" <TRANSFER_OBJECTS>
            "Transfer objects to the specified address."
        )
        .long_help(
            "Transfer objects to the specified address.\n\nExamples:\
            \n --transfer-objects @address [obj1, obj2, obj3]\
            \n --split-coins gas [1000, 5000, 75000]\
            \n --assign new_coins # bind new_coins to result of split-coins to use next\
            \n --transfer-objects @to_address [new_coins.0, new_coins.1, new_coins.2]"
        )
        .value_names(["TO", "[OBJECTS]"]))
        .arg(arg!(
            --"publish" <MOVE_PACKAGE_PATH>
            "Publish the move package. It takes as input the folder where the package exists."
        ).value_hint(ValueHint::DirPath))
        .arg(arg!(
            --"upgrade" <MOVE_PACKAGE_PATH>
            "Upgrade the move package. It takes as input the folder where the package exists."
        ).value_hint(ValueHint::DirPath))
        .arg(arg!(
            --"pick-gas-budget" <PICK_GAS_BUDGET>
            "Pick gas budget strategy if multiple gas-budgets are provided: \
            max (take the highest gas budget found) or sum (adding all provided gas budgets)"
        ))
        .arg(arg!(
            --"preview" 
            "Preview the list of PTB transactions instead of executing them."
        ))
        .arg(arg!(
            --"summary" 
            "Show only a short summary (digest, execution status, gas cost). \
            Do not use this flag when you need all the transaction data and the execution effects."
        ))
        .arg(arg!(
            --"warn-shadows" 
            "Enable shadow warning when including other PTB files. Off by default."
        ))
        .arg(arg!(
            --"json" 
            "Return command outputs in json format"
        ))
}
