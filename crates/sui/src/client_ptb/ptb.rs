// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::ptb_builder::errors::{PTBError, Span};
use crate::client_ptb::{
    displays::Pretty,
    ptb_builder::{
        build_ptb::PTBBuilder, command::ParsedPTBCommand, errors::render_errors,
        parse_ptb::PTBParser, parser::ProgramParser,
    },
};

use anyhow::{anyhow, Error};
use clap::{arg, Args};
use move_core_types::account_address::AccountAddress;
use petgraph::prelude::DiGraphMap;
use serde::Serialize;
use shared_crypto::intent::Intent;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponseOptions,
};
use sui_keys::keystore::AccountKeystore;
use sui_sdk::{wallet_context::WalletContext, SuiClient};
use sui_types::{
    base_types::ObjectID,
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

// /// The ProgrammableTransactionBlock structure used in the CLI ptb command
// #[derive(Parser, Debug, Default)]
// pub struct PTB {
//     /// The path to the file containing the PTBs
//     #[clap(long, num_args(1), required = false)]
//     file: Vec<String>,
//     /// An input for the PTB, defined as the variable name and value, e.g: --input recipient 0x321
//     #[clap(long, num_args(0..))]
//     assign: Vec<String>,
//     /// The object ID of the gas coin
//     #[clap(long, required = false)]
//     gas: String,
// }

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PTBCommand {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(clap::ValueEnum, Clone, Debug, Serialize, Default)]
pub enum PTBGas {
    #[default]
    Max,
    Sum,
}

pub struct PTBPreview {
    pub cmds: Vec<PTBCommand>,
}

#[derive(Serialize)]
pub struct Summary {
    pub digest: TransactionDigest,
    pub status: SuiExecutionStatus,
    pub gas_cost: GasCostSummary,
}

impl PTB {
    pub fn parse_args(
        &self,
        cwd: PathBuf,
        args: Vec<String>,
        included_files: &mut BTreeMap<PathBuf, Vec<PathBuf>>,
        output: &mut Vec<PTBCommand>,
    ) -> Result<(), Error> {
        let args = group_args_by_command(args);
        for arg in args.iter() {
            // skip empty arguments, e.g., spaces or json, summary, or gas
            if arg.is_empty() {
                continue;
            }
            // first is the command name, and then any values for that command
            // thus we slpit them at the first whitespace
            // if this is None, means we have a command without values (e.g., preview, summary, json, etc)
            match arg.split_once(' ') {
                None => {
                    output.push(PTBCommand {
                        name: arg.clone(),
                        values: vec!["true".to_string()],
                    });
                }
                Some((prefix, suffix)) => {
                    // we expect the values to become arguments, so we split into args
                    let args = Self::split_into_args(suffix.to_string())
                        .into_iter()
                        .map(|x| x.replace('\"', ""))
                        .filter(|x| !x.is_empty())
                        .collect::<Vec<_>>();
                    // handle the case of file inclusion first
                    if prefix == "file" {
                        // Things get complicated if multiple files are included at once, so
                        // let's restrict to one file at a time
                        if args.len() != 1 {
                            anyhow::bail!("Can only include one file at a time");
                        }
                        let current_file: PathBuf =
                            [cwd.clone(), Path::new(args.first().unwrap()).to_path_buf()]
                                .iter()
                                .collect();
                        output.push(PTBCommand {
                            name: "file-include-start".to_string(),
                            values: args.clone(),
                        });
                        self.resolve_file(
                            cwd.clone(),
                            args.clone(),
                            included_files,
                            current_file.clone(),
                            output,
                        )?;

                        // end of file inclusion
                        output.push(PTBCommand {
                            name: "file-include-end".to_string(),
                            values: args,
                        });
                    } else {
                        output.push(PTBCommand {
                            name: prefix.to_string(),
                            values: args,
                        })
                    }
                }
            }
        }
        Ok(())
    }

    pub fn preview(&self, commands: &[PTBCommand]) -> Option<PTBPreview> {
        // Preview the PTB instead of executing if preview flag is set
        let preview = commands.iter().any(|x| x.name == "preview");
        preview.then_some(PTBPreview {
            cmds: commands.to_owned(),
        })
    }

    /// Resolve the passed file into the existing array of PTB commands (output)
    /// It will flatly include the list of PTBCommands from the given file
    /// into the existing data holding the PTBs, and return the new index for the
    /// next command
    fn resolve_file(
        &self,
        cwd: PathBuf,
        filename: Vec<String>,
        included_files: &mut BTreeMap<PathBuf, Vec<PathBuf>>,
        current_file: PathBuf,
        output: &mut Vec<PTBCommand>,
    ) -> Result<(), Error> {
        if filename.len() != 1 {
            return Err(anyhow!("The --file options should only pass one filename"));
        }
        let filename = filename
            .first()
            .ok_or_else(|| anyhow!("Empty input file list."))?;
        let file_path = std::path::Path::new(&cwd).join(filename);
        // TODO we might want to figure out how to handle missing symlinks, as canonicalize will
        // error on a missing file. Prb we need to use path_abs.
        let file_path = std::fs::canonicalize(file_path)
            .map_err(|_| anyhow!("Cannot find the absolute path of this file {}", filename))?;
        if !file_path.exists() {
            return Err(anyhow!("{filename} does not exist"));
        }

        // NB: The following replacements are necessary. In order to handle quoted values in PTB
        // files (i.e., to support command-line syntax in PTB files), we need to handle escaped
        // quotes replacing them with the alternate syntax for inner strings ('), we then remove
        // any remaining quotes.
        let file_content = std::fs::read_to_string(file_path.clone())?
            .replace("\\\"", "'") // Handle escaped quotes \" and replace with '
            .replace('\\', ""); // Remove newlines

        let ignore_comments = file_content
            .lines()
            .filter(|x| !x.starts_with('#'))
            .collect::<Vec<_>>();
        if ignore_comments.iter().any(|x| x.contains('#')) {
            return Err(anyhow!(
                "Found inlined comments in file {filename}, which are not allowed. Only line comments are supported."
            ));
        }

        let parent_folder = if let Some(p) = file_path.parent() {
            p.to_path_buf()
        } else {
            std::env::current_dir().map_err(|_| anyhow!("Cannot get current working directory."))?
        };

        let mut files_to_resolve = vec![];
        for file in ignore_comments
            .iter()
            .filter(|x| x.starts_with("--file"))
            .flat_map(|x| x.split("--file"))
            .map(|x| x.trim())
        {
            let mut p = PathBuf::new();
            p.push(parent_folder.clone());
            p.push(file);
            let file = std::fs::canonicalize(p).map_err(|_| {
                anyhow!("{} includes file {} which does not exist.", filename, file)
            })?;
            files_to_resolve.push(file);
        }

        if let Some(files) = included_files.get_mut(&current_file) {
            files.extend(files_to_resolve);
        } else {
            included_files.insert(file_path, files_to_resolve);
        }

        check_for_cyclic_file_inclusions(included_files)?;
        let splits = Self::split_into_args(ignore_comments.join(" "))
            .into_iter()
            .map(|x| x.replace('\"', ""))
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>();

        self.parse_args(parent_folder, splits, included_files, output)?;
        Ok(())
    }

    pub async fn parse_and_build_ptb(
        &self,
        parsed: Vec<(Span, ParsedPTBCommand)>,
        context: &WalletContext,
        client: SuiClient,
    ) -> Result<(ProgrammableTransaction, u64, bool), Vec<PTBError>> {
        let starting_addresses = context
            .config
            .keystore
            .addresses_with_alias()
            .into_iter()
            .map(|(sa, alias)| (alias.alias.clone(), AccountAddress::from(*sa)))
            .collect();
        let mut builder = PTBBuilder::new(starting_addresses, client.read_api());

        for p in parsed.into_iter() {
            builder.handle_command(p).await;
        }

        builder.finish()
    }

    // This function is used to split the input string into arguments. We decide when we have a new
    // argument based on the different delimiters that we have. If we are inside of a delimiter and
    // we hit a space, we should keep going until we find the end of the delimiter and a space at
    // which point that is the end of the argument.
    fn split_into_args(s: String) -> Vec<String> {
        let mut res = vec![];
        let mut temp = String::new();

        // Argument delimiters that cannot span multiple arguments. We know if we are inside of one
        // of these that we need to keep going to finish the argument.
        let mut in_quotes = false;
        let mut in_ticks = false;
        let mut brackets = 0;
        let mut parens = 0;
        let mut hairpins = 0;

        for c in s.chars() {
            if c == '"' {
                in_quotes = !in_quotes;
            }
            if c == '\'' {
                in_ticks = !in_ticks;
            }

            // NB: At this point all string escapes have been normalized to single quotes.
            // When we are in a literal string being passed to the PTB, we do not look or count
            // delimiters.
            if !in_ticks {
                if c == '[' {
                    brackets += 1;
                }
                if c == ']' {
                    brackets -= 1;
                }

                if c == '(' {
                    parens += 1;
                }
                if c == ')' {
                    parens -= 1;
                }

                if c == '<' {
                    hairpins += 1;
                }
                if c == '>' {
                    hairpins -= 1;
                }
            }

            let is_delimiter = c == ' ' // Hit a whitspace
                && !in_quotes  // Not currently inside a quote
                && !in_ticks   // Not currently inside a string literal
                && brackets == 0 // Not currently inside a bracket (array/vector)
                && parens == 0  // Not curently inside parens (some)
                && hairpins == 0; // Not currently inside hairpins (generics)

            if is_delimiter {
                res.push(temp.clone());
                temp.clear();
            } else {
                temp.push(c);
            }
        }
        res.push(temp);
        res
    }

    pub fn parse_ptb_commands(
        &self,
        commands: Vec<PTBCommand>,
    ) -> Result<Vec<(Span, ParsedPTBCommand)>, Vec<PTBError>> {
        // Build the PTB
        let mut parser = PTBParser::new();
        for command in commands {
            parser.parse_command(command);
        }
        parser.finish()
    }

    /// Parses and executes the PTB with the sender as the current active address
    pub async fn execute(
        self,
        args: Vec<String>,
        context: &mut WalletContext,
    ) -> Result<(), Error> {
        // we handle these flags separately
        let s = self.args.join(" ");
        println!("{}", s);
        let x = ProgramParser::new(s).unwrap().parse();
        println!("{:#?}", x);
        todo!();
        let mut json = false;
        let mut summary_flag = false;
        let mut gas_coin = false;
        for a in args.iter() {
            if a.as_str() == "--json" {
                json = true;
            }
            if a.as_str() == "--gas-coin" {
                gas_coin = true;
            }
            if a.as_str() == "--summary" {
                summary_flag = true;
            }
        }
        let cwd =
            std::env::current_dir().map_err(|_| anyhow!("Cannot get the working directory."))?;
        let mut commands = Vec::new();
        self.parse_args(cwd, args, &mut BTreeMap::new(), &mut commands)?;
        let gas_coin = gas_coin
            .then_some({
                commands
                    .iter()
                    .find(|x| x.name == "gas")
                    .and_then(|x| x.values.first())
            })
            .flatten();

        let parsed_ptb_commands = match self.parse_ptb_commands(commands.clone()) {
            Err(errors) => {
                let suffix = if errors.len() > 1 { "s" } else { "" };
                let rendered = render_errors(commands, errors);
                eprintln!("Encountered error{suffix} when parsing PTB:");
                for e in rendered.iter() {
                    eprintln!("{:?}", e);
                }
                anyhow::bail!("Could not build PTB due to previous error{suffix}");
            }
            Ok(parsed) => parsed,
        };

        if let Some(ptb_preview) = self.preview(&commands) {
            println!("{}", ptb_preview);
            return Ok(());
        }

        // We need to resolve object IDs, so we need a fullnode to access
        let client = context.get_client().await?;
        let (ptb, budget, _preview) = match self
            .parse_and_build_ptb(parsed_ptb_commands, context, client)
            .await
        {
            Err(errors) => {
                let suffix = if errors.len() > 1 { "s" } else { "" };
                eprintln!("Encountered error{suffix} when building PTB:");
                let rendered = render_errors(commands, errors);
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
        let coins = if let Some(gas) = gas_coin {
            if !gas.starts_with("@0x") {
                return Err(anyhow!("Gas input error: to distinguish it from a hex value, please use @ in front of addresses or object IDs: @{gas}"));
            }
            context
                .get_object_ref(ObjectID::from_hex_literal(&gas[1..])?)
                .await?
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

        if json {
            let json_string = if summary_flag {
                serde_json::to_string_pretty(&serde_json::json!(summary))
                    .map_err(|_| anyhow!("Cannot serialize PTB result to json"))?
            } else {
                serde_json::to_string_pretty(&serde_json::json!(transaction_response))
                    .map_err(|_| anyhow!("Cannot serialize PTB result to json"))?
            };
            println!("{}", json_string);
        } else if summary_flag {
            println!("{}", Pretty(&summary));
        } else {
            println!("{}", transaction_response);
        }

        Ok(())
    }
}

/// Check for circular file inclusion.
/// It uses toposort algorithm and returns an error on finding a cycle,
/// describing which file includes a file that was already included.
fn check_for_cyclic_file_inclusions(
    included_files: &BTreeMap<PathBuf, Vec<PathBuf>>,
) -> Result<(), Error> {
    let edges = included_files.iter().flat_map(|(k, vs)| {
        let vs = vs.iter().map(|v| v.to_str().unwrap());
        std::iter::repeat(k.to_str().unwrap()).zip(vs)
    });

    let graph: DiGraphMap<_, ()> = edges.collect();
    let sort = petgraph::algo::toposort(&graph, None);
    sort.map_err(|node| {
        anyhow!(
            "Cannot have circular file inclusions. It appears that the issue is in the {} file",
            node.node_id()
        )
    })?;
    Ok(())
}

/// Clap will just give us a list of args and the values passed split by whitespace
/// This function groups them into command + values per string.
/// E.g. "--assign", "X", "5" becomes "--assign X 5"
/// We need to perform a special split of values' arguments based on specific logic,
/// thus this grouping is needed.
fn group_args_by_command(args: Vec<String>) -> Vec<String> {
    let mut new_args = vec![];
    let mut curr_string = vec![];
    for arg in args {
        if arg.starts_with("--") {
            new_args.push(curr_string.join(" "));
            curr_string.clear();
            // remove the -- prefix, and replace - with _ as clap would do
            curr_string.push(arg.replace("--", "").replace('-', "_"));
        } else {
            curr_string.push(arg);
        }
    }
    if !curr_string.is_empty() {
        new_args.push(curr_string.join(" "));
    }
    new_args
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

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
    };

    use crate::client_ptb::ptb::check_for_cyclic_file_inclusions;

    #[test]
    fn test_cyclic_inclusion() {
        let mut included_files: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();

        let p1 = Path::new("a");
        let p2 = Path::new("b");
        let p3 = Path::new("c");

        included_files.insert(p1.to_path_buf(), vec![p2.to_path_buf()]);
        included_files.insert(p2.to_path_buf(), vec![p3.to_path_buf()]);

        assert!(check_for_cyclic_file_inclusions(&included_files).is_ok());

        included_files.insert(p3.to_path_buf(), vec![p1.to_path_buf()]);
        assert!(check_for_cyclic_file_inclusions(&included_files).is_err());

        included_files.clear();
        included_files.insert(p1.to_path_buf(), vec![p1.to_path_buf()]);
        assert!(check_for_cyclic_file_inclusions(&included_files).is_err());
    }
}
