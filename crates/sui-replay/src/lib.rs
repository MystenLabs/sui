// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;

use crate::replay::LocalExec;
use std::str::FromStr;
use sui_config::node::ExpensiveSafetyCheckConfig;
use sui_types::digests::TransactionDigest;
use tracing::info;

mod replay;

#[derive(Parser, Clone)]
#[clap(rename_all = "kebab-case")]
pub enum ReplayToolCommand {
    #[clap(name = "tx")]
    ReplayTransaction {
        #[clap(long, short)]
        tx_digest: String,
        #[clap(long, short)]
        show_effects: bool,
    },

    #[clap(name = "checkpoints")]
    ReplayCheckpoints {
        #[clap(long, short)]
        start: u64,
        #[clap(long, short)]
        end: u64,
        #[clap(long, short)]
        terminate_early: bool,
    },

    #[clap(name = "report")]
    Report,
}

pub async fn execute_replay_command(
    rpc_url: String,
    safety_checks: bool,
    cmd: ReplayToolCommand,
) -> anyhow::Result<()> {
    let safety = if safety_checks {
        ExpensiveSafetyCheckConfig::new_enable_all()
    } else {
        ExpensiveSafetyCheckConfig::default()
    };
    match cmd {
        ReplayToolCommand::ReplayTransaction {
            tx_digest,
            show_effects,
        } => {
            let tx_digest = TransactionDigest::from_str(&tx_digest)?;
            info!("Executing tx: {}", tx_digest);
            let effects = LocalExec::new_from_fn_url(&rpc_url)
                .await?
                .init_for_execution()
                .await?
                .execute(&tx_digest, safety)
                .await?;

            if show_effects {
                println!("{:#?}", effects)
            }

            info!("Execution finished successfully. Local and on-chain effects match.");
        }
        ReplayToolCommand::ReplayCheckpoints {
            start,
            end,
            terminate_early,
        } => {
            let mut total_tx = 0;
            info!("Executing checkpoints starting at {}", start,);
            for checkpoint in start..=end {
                total_tx += LocalExec::new_from_fn_url(&rpc_url)
                    .await?
                    .init_for_execution()
                    .await?
                    .execute_all_in_checkpoint(checkpoint, safety.clone(), terminate_early)
                    .await?;
                if checkpoint % 10 == 0 {
                    info!(
                        "Executed {} checkpoints @ {} total transactions",
                        checkpoint - start + 1,
                        total_tx
                    );
                }
            }
            info!(
                "Executing checkpoints ended at {}. Ran {} total transactions",
                end, total_tx
            );
        }
        ReplayToolCommand::Report => {
            let mut lx = LocalExec::new_from_fn_url(&rpc_url).await?;
            let epoch_table = lx.protocol_ver_to_epoch_map().await?;

            // We need this for other activities in this session
            lx.current_protocol_version = *epoch_table.keys().peekable().last().unwrap();

            println!("  Protocol Version  |                Epoch Change TX               |      Epoch Range");
            println!("-------------------------------------------------------------------------------------");

            for (protocol_version, (tx_digest, start_epoch, end_epoch)) in epoch_table {
                println!(
                    " {:^16}   | {:^32} | {:^10}-{:^10}",
                    protocol_version, tx_digest, start_epoch, end_epoch
                );
            }

            lx.populate_protocol_version_tables().await?;
            for x in lx.protocol_version_system_package_table {
                println!("Protocol version: {}", x.0);
                for (package_id, seq_num) in x.1 {
                    println!("Package: {} Seq: {}", package_id, seq_num);
                }
            }
        }
    }
    Ok(())
}
