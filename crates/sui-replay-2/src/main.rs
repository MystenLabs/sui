// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use core::panic;
use similar::{ChangeTag, TextDiff};
use sui_json_rpc_types::SuiTransactionBlockEffects;
use sui_replay_2::{
    artifacts::{Artifact, ArtifactManager},
    build::handle_build_command,
    displays::Pretty,
    handle_replay_config, Commands, Config,
};
use sui_types::effects::TransactionEffects;
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
            let tx_digest = config.replay.digest.clone();
            let show_effects = config.replay.show_effects;

            let output_root = handle_replay_config(config.replay, VERSION)?;

            if let Some(digest) = tx_digest {
                let output_dir = output_root.join(&digest);
                let manager = ArtifactManager::new(&output_dir, false)?;
                if manager.member(Artifact::ForkedTransactionEffects).exists() {
                    println!("*** Transaction {digest} forked");
                    let forked_effects = manager
                        .member(Artifact::ForkedTransactionEffects)
                        .try_get_transaction_effects()
                        .transpose()?
                        .unwrap();
                    let expected_effects = manager
                        .member(Artifact::TransactionEffects)
                        .try_get_transaction_effects()
                        .transpose()?
                        .unwrap();
                    println!(
                        "*** Forked Transaction Effects for {digest}\n{}",
                        diff_effects(&expected_effects, &forked_effects)
                    );
                } else if show_effects {
                    let tx_effects = manager
                        .member(Artifact::TransactionEffects)
                        .try_get_transaction_effects()
                        .transpose()?
                        .unwrap();
                    println!(
                        "*** Transaction Effects for {digest}\n{}",
                        SuiTransactionBlockEffects::try_from(tx_effects.clone())
                            .map_err(|e| anyhow::anyhow!("Failed to convert effects: {e}"))?
                    );
                    manager
                        .member(Artifact::TransactionGasReport)
                        .try_get_gas_report()
                        .transpose()?
                        .map(|report| {
                            println!(
                                "*** Transaction Gas Report for {digest}\n{}",
                                Pretty(&report)
                            );
                        })
                        .unwrap_or_else(|| {
                            println!("*** No gas report available for transaction {digest}");
                        });
                }
            }
        }
    }
    Ok(())
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
