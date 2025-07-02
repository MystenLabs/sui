// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use anyhow::bail;
use clap::*;
use core::panic;
use move_trace_format::format::MoveTraceBuilder;
use similar::{ChangeTag, TextDiff};
use std::path::PathBuf;
use sui_replay_2::build::handle_build_command;
use sui_replay_2::{
    data_store::DataStore,
    execution::execute_transaction_to_effects,
    replay_txn::ReplayTransaction,
    tracing::{get_trace_output_path, save_trace_output},
    Commands, Config, ReplayConfig,
};
use sui_types::{effects::TransactionEffects, gas::SuiGasStatus};
use tracing::debug;

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

fn main() -> anyhow::Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let config = Config::parse();
    debug!("Parsed config: {:#?}", config);

    match config.command {
        Some(Commands::Build(build_config)) => {
            handle_build_command(build_config)?;
        }
        None => {
            // Default to replay behavior when no subcommand is specified
            handle_replay_command(config.replay)?;
        }
    }

    Ok(())
}

fn handle_replay_command(config: ReplayConfig) -> anyhow::Result<()> {
    let ReplayConfig {
        node,
        digest,
        digests_path,
        show_effects,
        verify,
        trace,
    } = config;

    // If a file is specified it is read and the digest ignored.
    // Once we decide on the options we want this is likely to change.
    let digests = if let Some(digests_path) = digests_path {
        // read digests from file
        std::fs::read_to_string(digests_path.clone())
            .map_err(|e| {
                anyhow!(
                    "Failed to read digests file {}: {e}",
                    digests_path.display(),
                )
            })?
            .lines()
            .map(|s| s.trim().to_string())
            .collect::<Vec<_>>()
    } else if let Some(tx_digest) = digest {
        // single digest provided
        vec![tx_digest]
    } else {
        bail!("either --digest or --digests-path must be provided");
    };

    debug!("Binary version: {VERSION}");

    // `DataStore` implements `TransactionStore`, `EpochStore` and `ObjectStore`
    let data_store = DataStore::new(node, VERSION)
        .map_err(|e| anyhow!("Failed to create data store: {:?}", e))?;

    // load and replay transactions
    for tx_digest in digests {
        replay_transaction(&tx_digest, &data_store, trace.clone(), show_effects, verify);
    }

    Ok(())
}

//
// Run a single transaction and print results to stdout
//
fn replay_transaction(
    tx_digest: &str,
    data_store: &DataStore,
    trace: Option<Option<PathBuf>>,
    show_effects: bool,
    verify: bool,
) {
    // load a `ReplayTranaction`
    let replay_txn = match ReplayTransaction::load(tx_digest, data_store, data_store, data_store) {
        Ok(replay_txn) => replay_txn,
        Err(e) => {
            println!("** TRANSACTION {} failed to load -> {:?}", tx_digest, e);
            return;
        }
    };

    // replay the transaction
    let mut trace_builder_opt = trace.clone().map(|_| MoveTraceBuilder::new());
    let (result, context_and_effects) = match execute_transaction_to_effects(
        replay_txn,
        data_store,
        data_store,
        &mut trace_builder_opt,
    ) {
        Ok((result, context_and_effects)) => (result, context_and_effects),
        Err(e) => {
            println!("** TRANSACTION {} failed to execute -> {:?}", tx_digest, e);
            return;
        }
    };

    // TODO: make tracing better abstracted? different tracers?
    if let Some(trace_builder) = trace_builder_opt {
        let _ = get_trace_output_path(trace.unwrap())
            .and_then(|output_path| save_trace_output(&output_path, tx_digest, trace_builder, &context_and_effects))
            .map_err(|e| {
                println!(
                    "WARNING (skipping tracing): transaction {} failed to build a trace output path -> {:?}",
                    tx_digest, e
                );
                e
            });
    };

    // print results
    println!("** TRANSACTION {} -> {:?}", tx_digest, result);
    if show_effects {
        print_txn_effects(
            &context_and_effects.execution_effects,
            &context_and_effects.gas_status,
        );
    }
    if verify {
        verify_txn(
            &context_and_effects.expected_effects,
            &context_and_effects.execution_effects,
        );
    }
}

//
// After command printing of requested results
//

fn print_txn_effects(effects: &TransactionEffects, gas_status: &SuiGasStatus) {
    println!("*** TRANSACTION EFFECTS -> {:?}", effects);
    println!("*** TRANSACTION GAS STATUS -> {:?}", gas_status);
}

fn verify_txn(expected_effects: &TransactionEffects, effects: &TransactionEffects) {
    println!("*** VERIFYING TRANSACTION EFFECTS");
    if effects != expected_effects {
        println!("**** FORKING: TRANSACTION EFFECTS DO NOT MATCH");
        println!("{}", diff_effects(expected_effects, effects));
    } else {
        println!("**** SUCCESS: TRANSACTION EFFECTS MATCH");
    }
}

/// Utility to diff `TransactionEffect` in a human readable format
fn diff_effects(expected_effect: &TransactionEffects, txn_effects: &TransactionEffects) -> String {
    let expected = format!("{:#?}", expected_effect);
    let result = format!("{:#?}", txn_effects);
    let mut res = vec![];

    let diff = TextDiff::from_lines(&expected, &result);
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "---",
            ChangeTag::Insert => "+++",
            ChangeTag::Equal => "   ",
        };
        res.push(format!("{}{}", sign, change));
    }

    res.join("")
}
