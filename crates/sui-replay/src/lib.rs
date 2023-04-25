// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_recursion::async_recursion;
use clap::Parser;

use crate::replay::LocalExec;
use crate::replay::ProtocolVersionSummary;
use std::str::FromStr;
use sui_config::node::ExpensiveSafetyCheckConfig;
use sui_types::digests::TransactionDigest;
use tracing::{error, info};
mod data_fetcher;
mod db_rider;
mod replay;
mod types;

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

    #[clap(name = "ch")]
    ReplayCheckpoints {
        #[clap(long, short)]
        start: u64,
        #[clap(long, short)]
        end: u64,
        #[clap(long, short)]
        terminate_early: bool,
        #[clap(long, short, default_value = "16")]
        max_tasks: u64,
    },

    #[clap(name = "ep")]
    ReplayEpoch {
        #[clap(long, short)]
        epoch: u64,
        #[clap(long, short)]
        terminate_early: bool,
        #[clap(long, short, default_value = "16")]
        max_tasks: u64,
    },

    #[clap(name = "report")]
    Report,
}

#[async_recursion]
pub async fn execute_replay_command(
    rpc_url: String,
    safety_checks: bool,
    cmd: ReplayToolCommand,
) -> anyhow::Result<(u64, u64)> {
    let safety = if safety_checks {
        ExpensiveSafetyCheckConfig::new_enable_all()
    } else {
        ExpensiveSafetyCheckConfig::default()
    };
    Ok(match cmd {
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
            (1u64, 1u64)
        }

        ReplayToolCommand::Report => {
            let mut lx = LocalExec::new_from_fn_url(&rpc_url).await?;
            let epoch_table = lx.protocol_ver_to_epoch_map().await?;

            // We need this for other activities in this session
            lx.current_protocol_version = *epoch_table.keys().peekable().last().unwrap();

            println!("  Protocol Version  |                Epoch Change TX               |      Epoch Range     |   Checkpoint Range   ");
            println!("---------------------------------------------------------------------------------------------------------------");

            for (
                protocol_version,
                ProtocolVersionSummary {
                    epoch_change_tx: tx_digest,
                    epoch_start: start_epoch,
                    epoch_end: end_epoch,
                    checkpoint_start,
                    checkpoint_end,
                    ..
                },
            ) in epoch_table
            {
                println!(
                    " {:^16}   | {:^43} | {:^10}-{:^10}| {:^10}-{:^10} ",
                    protocol_version,
                    tx_digest,
                    start_epoch,
                    end_epoch,
                    checkpoint_start,
                    checkpoint_end
                );
            }

            lx.populate_protocol_version_tables().await?;
            for x in lx.protocol_version_system_package_table {
                println!("Protocol version: {}", x.0);
                for (package_id, seq_num) in x.1 {
                    println!("Package: {} Seq: {}", package_id, seq_num);
                }
            }
            (0u64, 0u64)
        }

        ReplayToolCommand::ReplayCheckpoints {
            start,
            end,
            terminate_early,
            max_tasks,
        } => {
            assert!(start <= end, "Start checkpoint must be <= end checkpoint");
            assert!(max_tasks > 0, "Max tasks must be > 0");
            let checkpoints_per_task = ((end - start + max_tasks) / max_tasks) as usize;
            let mut handles = vec![];
            info!(
                "Executing checkpoints {} to {} with at most {} tasks and at most {} checkpoints per task",
                start, end, max_tasks, checkpoints_per_task
            );

            let range: Vec<_> = (start..=end).collect();
            for (task_count, checkpoints) in range.chunks(checkpoints_per_task).enumerate() {
                let checkpoints = checkpoints.to_vec();
                let rpc_url = rpc_url.clone();
                let safety = safety.clone();
                handles.push(tokio::spawn(async move {
                    info!("Spawning task {task_count} for checkpoints {checkpoints:?}");
                    let time = std::time::Instant::now();
                    let (succeeded, total) = LocalExec::new_from_fn_url(&rpc_url)
                        .await
                        .unwrap()
                        .init_for_execution()
                        .await
                        .unwrap()
                        .execute_all_in_checkpoints(&checkpoints, &safety, terminate_early)
                        .await
                        .unwrap();
                    let time = time.elapsed();
                    info!(
                        "Task {task_count}: executed checkpoints {:?} @ {} total transactions, {} succeeded",
                        checkpoints, total, succeeded
                    );
                    (succeeded, total, time)
                }));
            }

            let mut total_tx = 0;
            let mut total_time_ms = 0;
            let mut total_succeeded = 0;
            futures::future::join_all(handles)
                .await
                .into_iter()
                .for_each(|x| match x {
                    Ok((suceeded, total, time)) => {
                        total_tx += total;
                        total_time_ms += time.as_millis() as u64;
                        total_succeeded += suceeded;
                    }
                    Err(e) => {
                        error!("Task failed: {:?}", e);
                    }
                });
            info!(
                "Executed {} checkpoints @ {}/{} total TXs succeeded in {} ms ({}) avg TX/s",
                end - start + 1,
                total_succeeded,
                total_tx,
                total_time_ms,
                (total_tx as f64) / (total_time_ms as f64 / 1000.0)
            );
            (total_succeeded, total_tx)
        }
        ReplayToolCommand::ReplayEpoch {
            epoch,
            terminate_early,
            max_tasks,
        } => {
            let lx = LocalExec::new_from_fn_url(&rpc_url).await?;

            let (start, end) = lx.checkpoints_for_epoch(epoch).await?;

            info!(
                "Executing epoch {} (checkpoint range {}-{}) with at most {} tasks",
                epoch, start, end, max_tasks
            );
            let status = execute_replay_command(
                rpc_url,
                safety_checks,
                ReplayToolCommand::ReplayCheckpoints {
                    start,
                    end,
                    terminate_early,
                    max_tasks,
                },
            )
            .await;
            match status {
                Ok((succeeded, total)) => {
                    info!(
                        "Epoch {} replay finished {} out of {} TXs",
                        epoch, succeeded, total
                    );

                    return Ok((succeeded, total));
                }
                Err(e) => {
                    error!("Epoch {} replay failed: {:?}", epoch, e);
                    return Err(e);
                }
            }
        }
    })
}
