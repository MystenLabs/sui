// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ptb::ptb_builder::build_ptb::PTBBuilder;
use crate::ptb::ptb_builder::errors::render_errors;
use crate::ptb::ptb_builder::parse_ptb::PTBParser;
use anyhow::anyhow;
use anyhow::Error;
use clap::parser::ValuesRef;
use clap::ArgMatches;
use clap::CommandFactory;
use clap::Parser;
use move_core_types::account_address::AccountAddress;
use petgraph::prelude::DiGraphMap;
use serde::Serialize;
use shared_crypto::intent::Intent;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Display;
use std::fmt::Formatter;
use std::path::Path;
use std::path::PathBuf;
use sui_sdk::SuiClient;
use sui_types::base_types::ObjectID;
use sui_types::transaction::ProgrammableTransaction;

use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_keys::keystore::AccountKeystore;
use sui_sdk::wallet_context::WalletContext;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;

use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;

use tabled::{
    builder::Builder as TableBuilder,
    settings::{style::HorizontalLine, Panel as TablePanel, Style as TableStyle},
};

use super::ptb_builder::errors::PTBError;
use super::ptb_builder::parse_ptb::ParsedPTBCommand;

/// The ProgrammableTransactionBlock structure used in the CLI ptb command
#[derive(Parser, Debug, Default)]
pub struct PTB {
    /// The path to the file containing the PTBs
    #[clap(long, num_args(1), required = false)]
    file: Vec<String>,
    /// An input for the PTB, defined as the variable name and value, e.g: --input recipient 0x321
    #[clap(long, num_args(0..))]
    assign: Vec<String>,
    /// The object ID of the gas coin
    #[clap(long, required = false)]
    gas: String,
    /// The gas budget to be used to execute this PTB
    #[clap(long)]
    gas_budget: Option<String>,
    /// Given n-values of the same type, it constructs a vector.
    /// For non objects or an empty vector, the type tag must be specified.
    /// For example, --make-move-vec "<u64>" "[]"
    #[clap(long, num_args(1..))]
    make_move_vec: Vec<String>,
    /// Merge N coins into the provided coin: --merge-coins into_coin "[coin1,coin2,coin3]"
    #[clap(long, num_args(1..))]
    merge_coins: Vec<String>,
    /// Make a move call to a function
    #[clap(long, num_args(1..))]
    move_call: Vec<String>,
    /// Split the coin into N coins as per the given amount.
    /// On zsh, the vector needs to be given in quotes: --split-coins coin_to_split "[amount1,amount2]"
    #[clap(long, num_args(1..))]
    split_coins: Vec<String>,
    /// Transfer objects to the address. E.g., --transfer-objects to_address "[obj1, obj2]"
    #[clap(long, num_args(1..))]
    transfer_objects: Vec<String>,
    /// Publish the move package. It takes as input the folder where the package exists.
    #[clap(long, num_args(1..))]
    publish: Vec<String>,
    /// Upgrade the move package. It takes as input the folder where the package exists.
    #[clap(long, num_args(1..))]
    upgrade: Vec<String>,
    /// Preview the PTB instead of executing it
    #[clap(long)]
    preview: bool,
    /// Enable shadown warning when including other PTB files.
    /// Off by default.
    #[clap(long)]
    warn_shadows: bool,
    /// Pick gas budget strategy if multiple gas-budgets are provided.
    #[clap(long)]
    pick_gas_budget: Option<PTBGas>,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PTBCommand {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(clap::ValueEnum, Clone, Debug, Serialize, Default)]
enum PTBGas {
    MIN,
    #[default]
    MAX,
    SUM,
}

pub struct PTBPreview {
    cmds: Vec<PTBCommand>,
}

impl PTBCommand {
    fn is_preview_false(&self) -> bool {
        self.name == "preview" && self.values == ["false".to_string()]
    }
    fn is_warn_shadows_false(&self) -> bool {
        self.name == "warn_shadows" && self.values == ["false".to_string()]
    }
}

impl PTB {
    /// Get the passed arguments for this PTB and construct
    /// a map where the key is the command index,
    /// and the value is the name of the command and the values passed
    /// This is ordered as per how these args are given at the command line
    pub fn from_matches(
        &self,
        cwd: PathBuf,
        matches: &ArgMatches,
        included_files: &mut BTreeMap<PathBuf, Vec<PathBuf>>,
    ) -> Result<BTreeMap<usize, PTBCommand>, Error> {
        let mut order = BTreeMap::<usize, PTBCommand>::new();
        for arg_name in matches.ids() {
            if matches.try_get_many::<clap::Id>(arg_name.as_str()).is_ok() {
                continue;
            }

            // we need to skip the json as this is handled in the execute fn
            if arg_name.as_str() == "json" || arg_name.as_str() == "gas" {
                continue;
            }

            if arg_name.as_str() == "pick_gas_budget" {
                insert_value::<PTBGas>(arg_name, &matches, &mut order)?;
            } else if arg_name.as_str() == "preview" || arg_name.as_str() == "warn_shadows" {
                insert_value::<bool>(arg_name, &matches, &mut order)?;
            } else {
                insert_value::<String>(arg_name, &matches, &mut order)?;
            }
        }
        Ok(self.build_ptb_for_parsing(cwd, order, included_files)?)
    }

    /// Builds a sequential list of ptb commands that should be fed into the parser
    pub fn build_ptb_for_parsing(
        &self,
        cwd: PathBuf,
        ptb: BTreeMap<usize, PTBCommand>,
        included_files: &mut BTreeMap<PathBuf, Vec<PathBuf>>,
    ) -> Result<BTreeMap<usize, PTBCommand>, Error> {
        // the ptb input is a list of commands  and values, where the key is the index
        // of that value / command as it appearead in the args list on the CLI.
        // A command can have multiple values, and these values will appear sequential
        // with their indexes being consecutive (for that same command),
        // so we need to build the list of values for that specific command.
        // e.g., 1 [vals], 2 [vals], 4 [vals], 6 [vals], 1 + 2's value are
        // for the same command, 4 and 6 are different commands
        let mut output = BTreeMap::<usize, PTBCommand>::new();
        let mut curr_idx = 0;
        let mut cmd_idx = 0;

        for (idx, val) in ptb.iter() {
            // these bool commands do not take any values
            // so handle them separately
            if val.name == "preview" || val.name == "warn-shadows" {
                cmd_idx += 1;
                output.insert(cmd_idx, val.clone());
                continue;
            }

            // the current value is for the current command we're building
            // so add it to the output's value at key cmd_idx
            if idx == &(curr_idx + 1) {
                output
                    .get_mut(&cmd_idx)
                    .unwrap()
                    .values
                    .extend(val.values.clone());
                curr_idx += 1;
            } else {
                // we have a new command, so insert the value and increment curr_idx
                cmd_idx += 1;
                curr_idx = *idx;
                // check if the command is a file inclusion, as we need to sequentially
                // insert that in the array of PTBCommands
                if val.name == "file" {
                    let current_file: PathBuf = [
                        cwd.clone(),
                        Path::new(val.values.first().unwrap()).to_path_buf(),
                    ]
                    .iter()
                    .collect();
                    let new_index = self.resolve_file(
                        cwd.clone(),
                        val.values.clone(),
                        included_files,
                        current_file,
                        cmd_idx,
                        &mut output,
                    )?;
                    cmd_idx = new_index;
                } else {
                    output.insert(cmd_idx, val.clone());
                }
            }
        }
        Ok(output)
    }

    pub fn preview(&self, commands: &BTreeMap<usize, PTBCommand>) -> Option<PTBPreview> {
        // Preview the PTB instead of executing if preview flag is set
        let preview = commands
            .values()
            .find(|x| {
                x.name == "preview" && x.values.iter().find(|x| x.as_str() == "true").is_some()
            })
            .is_some();
        preview.then_some(PTBPreview {
            cmds: commands.clone().into_values().collect::<Vec<_>>(),
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
        start_index: usize,
        output: &mut BTreeMap<usize, PTBCommand>,
    ) -> Result<usize, Error> {
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
            .replace("\\", ""); // Remove newlines

        let ignore_comments = file_content
            .lines()
            .filter(|x| !x.starts_with("#"))
            .collect::<Vec<_>>();
        if ignore_comments.iter().any(|x| x.contains("#")) {
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

        check_for_cyclic_file_inclusions(&included_files)?;
        let splits = Self::split_into_args(ignore_comments.join(" "))
            .into_iter()
            .map(|x| x.replace("\"", ""))
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>();

        // in a file the first arg will not be the binary's name, so exclude it
        let input = PTB::command().no_binary_name(true);
        let args = input.get_matches_from(splits);
        let ptb_commands = self.from_matches(parent_folder, &args, included_files)?;
        let len_cmds = ptb_commands.len();

        // add a pseudo command to tag where does the file include start and end
        // this helps with returning errors as we need to point in which file, this occurs
        output.insert(
            start_index,
            PTBCommand {
                name: format!("file-include-start"),
                values: vec![filename.to_string()],
            },
        );
        for (k, v) in ptb_commands.into_iter() {
            output.insert(start_index + k, v);
        }

        // end of file inclusion
        output.insert(
            start_index + len_cmds + 1,
            PTBCommand {
                name: format!("file-include-end"),
                values: vec![filename.to_string()],
            },
        );

        Ok(start_index + len_cmds + 1)
    }

    pub async fn parse_and_build_ptb(
        &self,
        parsed: Vec<ParsedPTBCommand>,
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

    /// Parses and executes the PTB with the sender as the current active address
    pub async fn execute(
        self,
        matches: ArgMatches,
        context: Option<WalletContext>,
    ) -> Result<(), Error> {
        let ptb_args_matches = matches
            .subcommand_matches("client")
            .ok_or_else(|| anyhow!("Expected the client command but got a different command"))?
            .subcommand_matches("ptb")
            .ok_or_else(|| anyhow!("Expected the ptb subcommand but got a different command"))?;
        let json = ptb_args_matches.get_flag("json");
        let gas_coin = ptb_args_matches.get_one::<String>("gas");
        let cwd =
            std::env::current_dir().map_err(|_| anyhow!("Cannot get the working directory."))?;
        let commands = self.from_matches(cwd, ptb_args_matches, &mut BTreeMap::new())?;

        // If there are only 2 commands, they are likely the default
        // --preview and --warn-shadows set to false by clap,
        // so we can return early because there's no input
        if commands.len() == 2 {
            if let (Some(a), Some(b)) = (commands.get(&1), commands.get(&2)) {
                if a.name == "preview" && b.name == "warn_shadows" {
                    println!("No PTB to process. See the help menu for more information.");
                    return Ok(());
                }
            };
        }

        if let Some(ptb_preview) = &self.preview(&commands) {
            println!("{}", ptb_preview);
            return Ok(());
        }

        // Build the PTB
        let mut parser = PTBParser::new();
        for command in commands.clone() {
            parser.parse(command.1);
        }

        let parsed = match parser.finish() {
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

        // We need to resolve object IDs, so we need a fullnode to access
        let context = if let Some(context) = context {
            context
        } else {
            let config_path = sui_config::sui_config_dir()?.join(sui_config::SUI_CLIENT_CONFIG);
            let context = WalletContext::new(&config_path, None, None).await?;
            context
        };

        let client = context.get_client().await?;
        let (ptb, budget, _preview) = match self.parse_and_build_ptb(parsed, &context, client).await
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
        println!("Executing the transaction...");
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

        println!("Transaction executed");
        if json {
            let json_string =
                serde_json::to_string_pretty(&serde_json::json!(transaction_response))
                    .map_err(|_| anyhow!("Cannot serialize PTB result to json"))?;
            println!("{}", json_string);
        } else {
            println!("{}", transaction_response);
        }

        Ok(())
    }
}

impl Display for PTBPreview {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut builder = TableBuilder::default();
        let columns = vec!["command", "from", "value(s)"];
        builder.set_header(columns);
        let mut from = "console";
        for cmd in &self.cmds {
            if cmd.name == "file-include-start" {
                from = cmd.values.get(0).unwrap();
                continue;
            } else if cmd.name == "file-include-end" {
                from = "console";
                continue;
            } else if cmd.name == "preview" && cmd.is_preview_false() {
                continue;
            } else if cmd.name == "warn_shadows" && cmd.is_warn_shadows_false() {
                continue;
            }
            builder.push_record([
                cmd.name.to_string(),
                from.to_string(),
                cmd.values.join(" ").to_string(),
            ]);
        }
        let mut table = builder.build();
        table.with(TablePanel::header(format!("PTB Preview")));
        table.with(TableStyle::rounded().horizontals([
            HorizontalLine::new(1, TableStyle::modern().get_horizontal()),
            HorizontalLine::new(2, TableStyle::modern().get_horizontal()),
            HorizontalLine::new(2, TableStyle::modern().get_horizontal()),
        ]));
        table.with(tabled::settings::style::BorderSpanCorrection);

        write!(f, "{}", table)
    }
}

impl Display for PTBGas {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let r = match self {
            PTBGas::MIN => "min",
            PTBGas::MAX => "max",
            PTBGas::SUM => "sum",
        };
        write!(f, "{}", r.to_string())
    }
}

fn insert_value<T>(
    arg_name: &clap::Id,
    matches: &ArgMatches,
    order: &mut BTreeMap<usize, PTBCommand>,
) -> Result<(), Error>
where
    T: Clone + Display + Send + Sync + 'static,
{
    let values: ValuesRef<'_, T> = matches
        .get_many(arg_name.as_str())
        .ok_or_else(|| anyhow!("Cannot parse the args for the PTB"))?;
    for (value, index) in values.zip(
        matches
            .indices_of(arg_name.as_str())
            .expect("id came from matches"),
    ) {
        order.insert(
            index,
            PTBCommand {
                name: arg_name.to_string(),
                values: vec![value.to_string()],
            },
        );
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
    };

    use crate::ptb::ptb::check_for_cyclic_file_inclusions;

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
