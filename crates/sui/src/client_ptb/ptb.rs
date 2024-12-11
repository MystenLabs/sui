// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    client_commands::{dry_run_or_execute_or_serialize, Opts, OptsWithGas, SuiClientCommandResult},
    client_ptb::{
        ast::{ParsedProgram, Program},
        builder::PTBBuilder,
        error::{build_error_reports, PTBError},
        token::{Lexeme, Token},
    },
    displays::Pretty,
    sp,
};

use super::{ast::ProgramMetadata, lexer::Lexer, parser::ProgramParser};
use anyhow::{anyhow, ensure, Error};
use clap::{arg, Args, ValueHint};
use move_core_types::account_address::AccountAddress;
use serde::Serialize;
use sui_json_rpc_types::{SuiExecutionStatus, SuiTransactionBlockEffectsAPI};
use sui_keys::keystore::AccountKeystore;
use sui_sdk::{wallet_context::WalletContext, SuiClient};
use sui_types::{
    digests::TransactionDigest,
    gas::GasCostSummary,
    transaction::{ProgrammableTransaction, TransactionKind},
};

#[derive(Clone, Debug, Args)]
#[clap(disable_help_flag = true)]
pub struct PTB {
    #[clap(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

pub struct PTBPreview<'a> {
    pub program: &'a Program,
    pub program_metadata: &'a ProgramMetadata,
}

#[derive(Serialize)]
pub struct Summary {
    pub digest: TransactionDigest,
    pub status: SuiExecutionStatus,
    pub gas_cost: GasCostSummary,
}

impl PTB {
    /// Parses and executes the PTB with the sender as the current active address
    pub async fn execute(self, context: &mut WalletContext) -> Result<(), Error> {
        if self.args.is_empty() {
            ptb_description().print_help().unwrap();
            return Ok(());
        }
        let source_string = to_source_string(self.args.clone());

        // Tokenize once to detect help flags
        let tokens = self.args.iter().map(|s| s.as_str());
        for sp!(_, lexeme) in Lexer::new(tokens.clone()).into_iter().flatten() {
            match lexeme {
                Lexeme(Token::Command, "help") => return Ok(ptb_description().print_long_help()?),
                Lexeme(Token::Flag, "h") => return Ok(ptb_description().print_help()?),
                lexeme if lexeme.is_terminal() => break,
                _ => continue,
            }
        }

        // Tokenize and parse to get the program
        let (program, program_metadata) = match ProgramParser::new(tokens)
            .map_err(|e| vec![e])
            .and_then(|parser| parser.parse())
        {
            Err(errors) => {
                let suffix = if errors.len() > 1 { "s" } else { "" };
                let rendered = build_error_reports(&source_string, errors);
                eprintln!("Encountered error{suffix} when parsing PTB:");
                for e in rendered.iter() {
                    eprintln!("{:?}", e);
                }
                anyhow::bail!("Could not build PTB due to previous error{suffix}");
            }
            Ok(parsed) => parsed,
        };

        ensure!(
            !program_metadata.serialize_unsigned_set || !program_metadata.serialize_signed_set,
            "Cannot specify both flags: --serialize-unsigned-transaction and --serialize-signed-transaction."
        );

        if program_metadata.preview_set {
            println!(
                "{}",
                PTBPreview {
                    program: &program,
                    program_metadata: &program_metadata
                }
            );
            return Ok(());
        }

        let client = context.get_client().await?;

        let (res, warnings) = Self::build_ptb(program, context, client).await;

        // Render warnings
        if !warnings.is_empty() {
            let suffix = if warnings.len() > 1 { "s" } else { "" };
            eprintln!("Warning{suffix} produced when building PTB:");
            let rendered = build_error_reports(&source_string, warnings);
            for e in rendered.iter() {
                eprintln!("{:?}", e);
            }
        }
        let ptb = match res {
            Err(errors) => {
                let suffix = if errors.len() > 1 { "s" } else { "" };
                eprintln!("Encountered error{suffix} when building PTB:");
                let rendered = build_error_reports(&source_string, errors);
                for e in rendered.iter() {
                    eprintln!("{:?}", e);
                }
                anyhow::bail!("Could not build PTB due to previous error{suffix}");
            }
            Ok(x) => x,
        };

        // get all the metadata needed for executing the PTB: sender, gas, signing tx
        let gas = program_metadata.gas_object_id.map(|x| x.value);

        // the sender is the gas object if gas is provided, otherwise the active address
        let sender = match gas {
            Some(gas) => context
                .get_object_owner(&gas)
                .await
                .map_err(|_| anyhow!("Could not find owner for gas object ID"))?,
            None => context
                .config
                .active_address
                .ok_or_else(|| anyhow!("No active address, cannot execute PTB"))?,
        };

        // build the tx kind
        let tx_kind = TransactionKind::ProgrammableTransaction(ProgrammableTransaction {
            inputs: ptb.inputs,
            commands: ptb.commands,
        });

        let opts = OptsWithGas {
            gas: program_metadata.gas_object_id.map(|x| x.value),
            rest: Opts {
                dry_run: program_metadata.dry_run_set,
                dev_inspect: program_metadata.dev_inspect_set,
                gas_budget: program_metadata.gas_budget.map(|x| x.value),
                serialize_unsigned_transaction: program_metadata.serialize_unsigned_set,
                serialize_signed_transaction: program_metadata.serialize_signed_set,
            },
        };

        let transaction_response = dry_run_or_execute_or_serialize(
            sender, tx_kind, context, None, None, opts.gas, opts.rest,
        )
        .await?;

        let transaction_response = match transaction_response {
            SuiClientCommandResult::DryRun(_) => {
                println!("{}", transaction_response);
                return Ok(());
            }
            SuiClientCommandResult::SerializedUnsignedTransaction(_)
            | SuiClientCommandResult::SerializedSignedTransaction(_) => {
                println!("{}", transaction_response);
                return Ok(());
            }
            SuiClientCommandResult::TransactionBlock(response) => response,
            SuiClientCommandResult::DevInspect(response) => {
                println!("{}", Pretty(&response));
                return Ok(());
            }
            _ => anyhow::bail!("Internal error, unexpected response from PTB execution."),
        };

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
    ) -> (
        Result<ProgrammableTransaction, Vec<PTBError>>,
        Vec<PTBError>,
    ) {
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
    pub fn parse_ptb_commands(args: Vec<String>) -> Result<ParsedProgram, Vec<PTBError>> {
        ProgramParser::new(args.iter().map(|s| s.as_str()))
            .map_err(|e| vec![e])
            .and_then(|parser| parser.parse())
    }
}

/// Convert a vector of shell tokens into a single string, with each shell token separated by a
/// space with each command starting on a new line.
/// NB: we add a space to the end of the source string to ensure that for unexpected EOF
/// errors we have a location to point to.
pub fn to_source_string(strings: Vec<String>) -> String {
    let mut strings = strings.into_iter();
    let mut string = String::new();

    let Some(first) = strings.next() else {
        return string;
    };
    string.push_str(&first);

    for s in strings {
        if s.starts_with("--") {
            string.push('\n');
            string.push_str(&s);
        } else {
            string.push(' ');
            string.push_str(&s);
        }
    }
    string.push(' ');
    string
}

pub fn ptb_description() -> clap::Command {
    clap::Command::new("sui client ptb")
        .about(
            "Build, preview, and execute programmable transaction blocks. Depending on your \
            shell, you might have to use quotes around arrays or other passed values. \
            Use --help to see examples for how to use the core functionality of this command.")
        .arg(arg!(
                --"assign" <ASSIGN>
                "Assign a value to a variable name to use later in the PTB."
        )
        .long_help(
            "Assign a value to a variable name to use later in the PTB. \
            \n If only a name is supplied, the result of \
            the last transaction is bound to that name.\
            \n If a name and value are \
            supplied, then the name is bound to that value. \
            \n\nExamples: \
            \n --assign MYVAR 100 \
            \n --assign X [100,5000] \
            \n --split-coins gas [1000, 5000, 75000] \
            \n --assign new_coins # bound new_coins to the result of previous transaction"
        )
        .value_names(["NAME", "VALUE"]))
        .arg(arg!(
            --"dry-run"
            "Perform a dry run of the PTB instead of executing it."
        ))
        .arg(arg!(
            --"dev-inspect"
            "Perform a dev-inspect of the PTB instead of executing it."
        ))
        .arg(arg!(
            --"gas-coin" <ID> ...
            "The object ID of the gas coin to use. If not specified, it will try to use the first \
            gas coin that it finds that has at least the requested gas-budget balance."
        ))
        .arg(arg!(
            --"gas-budget" <MIST>
            "An optional gas budget for this PTB (in MIST). If gas budget is not provided, the \
            tool will first perform a dry run to estimate the gas cost, and then it will execute \
            the transaction. Please note that this incurs a small cost in performance due to the \
            additional dry run call."
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
            "Make a move call to a function.\
            \n\nExamples:\
            \n --move-call std::option::is_none <u64> none\
            \n --assign a none\
            \n --move-call std::option::is_none <u64> a"
        )
        .value_names(["PACKAGE::MODULE::FUNCTION", "TYPE_ARGS", "FUNCTION_ARGS"]))
        .arg(arg!(
            --"split-coins" <SPLIT_COINS>
            "Split the coin into N coins as per the given array of amounts."
        )
        .long_help(
            "Split the coin into N coins as per the given array of amounts.\
            \n\nExamples:\
            \n --split-coins gas [1000, 5000, 75000]\
            \n --assign new_coins # bounds the result of split-coins command to variable new_coins\
            \n --split-coins @coin_object_id [100]"
        )
        .value_names(["COIN", "[AMOUNT]"]))
        .arg(arg!(
            --"transfer-objects" <TRANSFER_OBJECTS>
            "Transfer objects to the specified address."
        )
        .long_help(
            "Transfer objects to the specified address.\
            \n\nExamples:\
            \n --transfer-objects [obj1, obj2, obj3] @address
            \n --split-coins gas [1000, 5000, 75000]\
            \n --assign new_coins # bound new_coins to result of split-coins to use next\
            \n --transfer-objects [new_coins.0, new_coins.1, new_coins.2] @to_address"
        )
        .value_names(["[OBJECTS]", "TO"]))
        .arg(arg!(
            --"publish" <MOVE_PACKAGE_PATH>
            "Publish the Move package. It takes as input the folder where the package exists."
        ).long_help(
            "Publish the Move package. It takes as input the folder where the package exists.\
            \n\nExamples:\
            \n --move-call sui::tx_context::sender\
            \n --assign sender\
            \n --publish \".\"\
            \n --assign upgrade_cap\
            \n --transfer-objects sender \"[upgrade_cap]\""
        ).value_hint(ValueHint::DirPath))
        .arg(arg!(
            --"upgrade" <MOVE_PACKAGE_PATH>
            "Upgrade the Move package. It takes as input the folder where the package exists."
        ).value_hint(ValueHint::DirPath))
        .arg(arg!(
            --"preview"
            "Preview the list of PTB transactions instead of executing them."
        ))
        .arg(arg!(
            --"serialize-unsigned-transaction"
            "Instead of executing the transaction, serialize the bcs bytes of the unsigned \
            transaction data using base64 encoding."
        ))
        .arg(arg!(
            --"serialize-signed-transaction"
            "Instead of executing the transaction, serialize the bcs bytes of the signed \
            transaction data using base64 encoding."
        ))
        .arg(arg!(
            --"summary"
            "Show only a short summary (digest, execution status, gas cost). \
            Do not use this flag when you need all the transaction data and the execution effects."
        ))
        .arg(arg!(
            --"warn-shadows"
            "Enable shadow warning when the same variable name is declared multiple times. Off by default."
        ))
        .arg(arg!(
            --"json"
            "Return command outputs in json format."
        ))
}
