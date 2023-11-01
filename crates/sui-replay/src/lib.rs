// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_recursion::async_recursion;
use clap::Parser;
use config::ReplayableNetworkConfigSet;
use fuzz::ReplayFuzzer;
use fuzz::ReplayFuzzerConfig;
use fuzz_mutations::base_fuzzers;
use move_vm_config::runtime::DEFAULT_PROFILE_OUTPUT_PATH;
use sui_types::digests::get_mainnet_chain_identifier;
use sui_types::digests::get_testnet_chain_identifier;
use sui_types::message_envelope::Message;
use tracing::warn;
use transaction_provider::{FuzzStartPoint, TransactionSource};

use crate::replay::ExecutionSandboxState;
use crate::replay::LocalExec;
use crate::replay::ProtocolVersionSummary;
use std::env;
use std::io::BufRead;
use std::path::PathBuf;
use std::str::FromStr;
use sui_config::node::ExpensiveSafetyCheckConfig;
use sui_protocol_config::Chain;
use sui_types::digests::TransactionDigest;
use tracing::{error, info};
pub mod config;
mod data_fetcher;
pub mod fuzz;
pub mod fuzz_mutations;
mod replay;
pub mod transaction_provider;
pub mod types;

static DEFAULT_SANDBOX_BASE_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/sandbox_snapshots");

#[cfg(test)]
mod tests;

#[derive(Parser, Clone)]
#[command(rename_all = "kebab-case")]
pub enum ReplayToolCommand {
    /// Generate a new network config file
    #[command(name = "gen")]
    GenerateDefaultConfig,

    /// Persist sandbox state
    #[command(name = "ps")]
    PersistSandbox {
        #[arg(long, short)]
        tx_digest: String,
        #[arg(long, short, default_value = DEFAULT_SANDBOX_BASE_PATH)]
        base_path: PathBuf,
    },

    /// Replay from sandbox state file
    /// This is a completely local execution
    #[command(name = "rs")]
    ReplaySandbox {
        #[arg(long, short)]
        path: PathBuf,
    },

    /// Replay transaction
    #[command(name = "rp")]
    ProfileTransaction {
        #[arg(long, short)]
        tx_digest: String,
        #[arg(long, short)]
        show_effects: bool,
        #[arg(long, short)]
        diag: bool,
        #[arg(long, short, allow_hyphen_values = true)]
        executor_version_override: Option<i64>,
        #[arg(long, short, allow_hyphen_values = true)]
        protocol_version_override: Option<i64>,
        #[arg(long, short, allow_hyphen_values = true)]
        profile_output_filepath_override: Option<PathBuf>,
    },

    /// Replay transaction
    #[command(name = "tx")]
    ReplayTransaction {
        #[arg(long, short)]
        tx_digest: String,
        #[arg(long, short)]
        show_effects: bool,
        #[arg(long, short)]
        diag: bool,
        #[arg(long, short, allow_hyphen_values = true)]
        executor_version_override: Option<i64>,
        #[arg(long, short, allow_hyphen_values = true)]
        protocol_version_override: Option<i64>,
    },

    /// Replay transactions listed in a file
    #[command(name = "rb")]
    ReplayBatch {
        #[arg(long, short)]
        path: PathBuf,
        #[arg(long, short)]
        terminate_early: bool,
        #[arg(long, short, default_value = "16")]
        batch_size: u64,
    },

    /// Replay a transaction from a node state dump
    #[command(name = "rd")]
    ReplayDump {
        #[arg(long, short)]
        path: String,
        #[arg(long, short)]
        show_effects: bool,
    },

    /// Replay all transactions in a range of checkpoints
    #[command(name = "ch")]
    ReplayCheckpoints {
        #[arg(long, short)]
        start: u64,
        #[arg(long, short)]
        end: u64,
        #[arg(long, short)]
        terminate_early: bool,
        #[arg(long, short, default_value = "16")]
        max_tasks: u64,
    },

    /// Replay all transactions in an epoch
    #[command(name = "ep")]
    ReplayEpoch {
        #[arg(long, short)]
        epoch: u64,
        #[arg(long, short)]
        terminate_early: bool,
        #[arg(long, short, default_value = "16")]
        max_tasks: u64,
    },

    /// Run the replay based fuzzer
    #[command(name = "fz")]
    Fuzz {
        #[arg(long, short)]
        start: Option<FuzzStartPoint>,
        #[arg(long, short)]
        num_mutations_per_base: u64,
        #[arg(long, short = 'b', default_value = "18446744073709551614")]
        num_base_transactions: u64,
    },

    #[command(name = "report")]
    Report,
}

#[async_recursion]
pub async fn execute_replay_command(
    rpc_url: Option<String>,
    safety_checks: bool,
    use_authority: bool,
    cfg_path: Option<PathBuf>,
    cmd: ReplayToolCommand,
) -> anyhow::Result<Option<(u64, u64)>> {
    let safety = if safety_checks {
        ExpensiveSafetyCheckConfig::new_enable_all()
    } else {
        ExpensiveSafetyCheckConfig::default()
    };
    Ok(match cmd {
        ReplayToolCommand::ReplaySandbox { path } => {
            let contents = std::fs::read_to_string(path)?;
            let sandbox_state: ExecutionSandboxState = serde_json::from_str(&contents)?;
            info!("Executing tx: {}", sandbox_state.transaction_info.tx_digest);
            let sandbox_state = LocalExec::certificate_execute_with_sandbox_state(
                &sandbox_state,
                None,
                &sandbox_state.pre_exec_diag,
            )
            .await?;
            sandbox_state.check_effects()?;
            info!("Execution finished successfully. Local and on-chain effects match.");
            None
        }
        ReplayToolCommand::PersistSandbox {
            tx_digest,
            base_path,
        } => {
            let tx_digest = TransactionDigest::from_str(&tx_digest)?;
            info!("Executing tx: {}", tx_digest);
            let sandbox_state = LocalExec::replay_with_network_config(
                rpc_url,
                cfg_path.map(|p| p.to_str().unwrap().to_string()),
                tx_digest,
                safety,
                use_authority,
                None,
                None,
                None,
            )
            .await?;

            let out = serde_json::to_string(&sandbox_state).unwrap();
            let path = base_path.join(format!("{}.json", tx_digest));
            std::fs::write(path, out)?;
            None
        }
        ReplayToolCommand::GenerateDefaultConfig => {
            let set = ReplayableNetworkConfigSet::default();
            let path = set.save_config(None).unwrap();
            println!("Default config saved to: {}", path.to_str().unwrap());
            warn!("Note: default config nodes might prune epochs/objects");
            None
        }
        ReplayToolCommand::Fuzz {
            start,
            num_mutations_per_base,
            num_base_transactions,
        } => {
            let config = ReplayFuzzerConfig {
                num_mutations_per_base,
                mutator: Box::new(base_fuzzers(num_mutations_per_base)),
                tx_source: TransactionSource::TailLatest { start },
                fail_over_on_err: false,
                expensive_safety_check_config: Default::default(),
            };
            let fuzzer = ReplayFuzzer::new(rpc_url.expect("Url must be provided"), config)
                .await
                .unwrap();
            fuzzer.run(num_base_transactions).await.unwrap();
            None
        }
        ReplayToolCommand::ReplayDump { path, show_effects } => {
            let mut lx = LocalExec::new_for_state_dump(&path, rpc_url).await?;
            let (sandbox_state, node_dump_state) = lx.execute_state_dump(safety).await?;
            if show_effects {
                println!("{:#?}", sandbox_state.local_exec_effects);
            }

            sandbox_state.check_effects()?;

            let effects = node_dump_state.computed_effects.digest();
            if effects != node_dump_state.expected_effects_digest {
                error!(
                    "Effects digest mismatch for {}: expected: {:?}, got: {:?}",
                    node_dump_state.tx_digest, node_dump_state.expected_effects_digest, effects,
                );
                anyhow::bail!("Effects mismatch");
            }

            info!("Execution finished successfully. Local and on-chain effects match.");
            Some((1u64, 1u64))
        }
        ReplayToolCommand::ReplayBatch {
            path,
            terminate_early,
            batch_size,
        } => {
            async fn exec_batch(
                rpc_url: Option<String>,
                safety: ExpensiveSafetyCheckConfig,
                use_authority: bool,
                cfg_path: Option<PathBuf>,
                tx_digests: &[TransactionDigest],
            ) -> anyhow::Result<()> {
                let mut handles = vec![];
                for tx_digest in tx_digests {
                    let tx_digest = *tx_digest;
                    let rpc_url = rpc_url.clone();
                    let cfg_path = cfg_path.clone();
                    let safety = safety.clone();
                    handles.push(tokio::spawn(async move {
                        info!("Executing tx: {}", tx_digest);
                        let sandbox_state = LocalExec::replay_with_network_config(
                            rpc_url,
                            cfg_path.map(|p| p.to_str().unwrap().to_string()),
                            tx_digest,
                            safety,
                            use_authority,
                            None,
                            None,
                            None,
                        )
                        .await?;

                        sandbox_state.check_effects()?;

                        info!("Execution finished successfully: {}. Local and on-chain effects match.", tx_digest);
                        Ok::<_, anyhow::Error>(())
                    }));
                }
                futures::future::join_all(handles)
                    .await
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                    .expect("Join all failed")
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(())
            }

            // While file end not reached, read up to max_tasks lines from path
            let file = std::fs::File::open(path).unwrap();
            let reader = std::io::BufReader::new(file);

            let mut chunk = Vec::new();
            for tx_digest in reader.lines() {
                chunk.push(
                    match TransactionDigest::from_str(&tx_digest.expect("Unable to readline")) {
                        Ok(digest) => digest,
                        Err(e) => {
                            panic!("Error parsing tx digest: {:?}", e);
                        }
                    },
                );
                if chunk.len() == batch_size as usize {
                    println!("Executing batch: {:?}", chunk);
                    // execute all in chunk
                    match exec_batch(
                        rpc_url.clone(),
                        safety.clone(),
                        use_authority,
                        cfg_path.clone(),
                        &chunk,
                    )
                    .await
                    {
                        Ok(_) => info!("Batch executed successfully: {:?}", chunk),
                        Err(e) => {
                            error!("Error executing batch: {:?}", e);
                            if terminate_early {
                                return Err(e);
                            }
                        }
                    }
                    println!("Finished batch execution");

                    chunk.clear();
                }
            }
            if !chunk.is_empty() {
                println!("Executing batch: {:?}", chunk);
                match exec_batch(
                    rpc_url.clone(),
                    safety,
                    use_authority,
                    cfg_path.clone(),
                    &chunk,
                )
                .await
                {
                    Ok(_) => info!("Batch executed successfully: {:?}", chunk),
                    Err(e) => {
                        error!("Error executing batch: {:?}", e);
                        if terminate_early {
                            return Err(e);
                        }
                    }
                }
                println!("Finished batch execution");
            }

            // TODO: clean this up
            Some((0u64, 0u64))
        }
        ReplayToolCommand::ProfileTransaction {
            tx_digest,
            show_effects,
            diag,
            executor_version_override,
            protocol_version_override,
            profile_output_filepath_override,
        } => {
            let tx_digest = TransactionDigest::from_str(&tx_digest)?;
            info!("Executing tx: {}", tx_digest);
            let sandbox_state = LocalExec::replay_with_network_config(
                rpc_url,
                cfg_path.map(|p| p.to_str().unwrap().to_string()),
                tx_digest,
                safety,
                use_authority,
                executor_version_override,
                protocol_version_override,
                profile_output_filepath_override
                    .or(Some((*DEFAULT_PROFILE_OUTPUT_PATH.clone()).to_path_buf())),
            )
            .await?;

            if diag {
                println!("{:#?}", sandbox_state.pre_exec_diag);
            }
            if show_effects {
                println!("{}", sandbox_state.local_exec_effects);
            }

            sandbox_state.check_effects()?;

            println!("Execution finished successfully. Local and on-chain effects match.");
            Some((1u64, 1u64))
        }

        ReplayToolCommand::ReplayTransaction {
            tx_digest,
            show_effects,
            diag,
            executor_version_override,
            protocol_version_override,
        } => {
            let tx_digest = TransactionDigest::from_str(&tx_digest)?;
            info!("Executing tx: {}", tx_digest);
            let sandbox_state = LocalExec::replay_with_network_config(
                rpc_url,
                cfg_path.map(|p| p.to_str().unwrap().to_string()),
                tx_digest,
                safety,
                use_authority,
                executor_version_override,
                protocol_version_override,
                None,
            )
            .await?;

            if diag {
                println!("{:#?}", sandbox_state.pre_exec_diag);
            }
            if show_effects {
                println!("{}", sandbox_state.local_exec_effects);
            }

            sandbox_state.check_effects()?;

            println!("Execution finished successfully. Local and on-chain effects match.");
            Some((1u64, 1u64))
        }

        ReplayToolCommand::Report => {
            let mut lx =
                LocalExec::new_from_fn_url(&rpc_url.expect("Url must be provided")).await?;
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
                    checkpoint_start.unwrap_or(u64::MAX),
                    checkpoint_end.unwrap_or(u64::MAX)
                );
            }

            lx.populate_protocol_version_tables().await?;
            for x in lx.protocol_version_system_package_table {
                println!("Protocol version: {}", x.0);
                for (package_id, seq_num) in x.1 {
                    println!("Package: {} Seq: {}", package_id, seq_num);
                }
            }
            None
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
                    let (succeeded, total) = LocalExec::new_from_fn_url(&rpc_url.expect("Url must be provided"))
                        .await
                        .unwrap()
                        .init_for_execution()
                        .await
                        .unwrap()
                        .execute_all_in_checkpoints(&checkpoints, &safety, terminate_early, use_authority)
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
            Some((total_succeeded, total_tx))
        }
        ReplayToolCommand::ReplayEpoch {
            epoch,
            terminate_early,
            max_tasks,
        } => {
            let lx =
                LocalExec::new_from_fn_url(&rpc_url.clone().expect("Url must be provided")).await?;

            let (start, end) = lx.checkpoints_for_epoch(epoch).await?;

            info!(
                "Executing epoch {} (checkpoint range {}-{}) with at most {} tasks",
                epoch, start, end, max_tasks
            );
            let status = execute_replay_command(
                rpc_url,
                safety_checks,
                use_authority,
                cfg_path,
                ReplayToolCommand::ReplayCheckpoints {
                    start,
                    end,
                    terminate_early,
                    max_tasks,
                },
            )
            .await;
            match status {
                Ok(Some((succeeded, total))) => {
                    info!(
                        "Epoch {} replay finished {} out of {} TXs",
                        epoch, succeeded, total
                    );

                    return Ok(Some((succeeded, total)));
                }
                Ok(None) => {
                    return Ok(None);
                }
                Err(e) => {
                    error!("Epoch {} replay failed: {:?}", epoch, e);
                    return Err(e);
                }
            }
        }
    })
}

pub(crate) fn chain_from_chain_id(chain: &str) -> Chain {
    let mainnet_chain_id = format!("{}", get_mainnet_chain_identifier());
    // TODO: Since testnet periodically resets, we need to ensure that the chain id
    // is updated to the latest one.
    let testnet_chain_id = format!("{}", get_testnet_chain_identifier());

    if mainnet_chain_id == chain {
        Chain::Mainnet
    } else if testnet_chain_id == chain {
        Chain::Testnet
    } else {
        Chain::Unknown
    }
}
