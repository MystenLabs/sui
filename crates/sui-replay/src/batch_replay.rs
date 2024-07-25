// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::replay::{ExecutionSandboxState, LocalExec};
use crate::types::ReplayEngineError;
use futures::future::join_all;
use futures::FutureExt;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use sui_config::node::ExpensiveSafetyCheckConfig;
use sui_types::base_types::TransactionDigest;
use tokio::time::Instant;
use tracing::{error, info};

/// Given a list of transaction digests, replay them in parallel using `num_tasks` tasks.
/// If `terminate_early` is true, the replay will terminate early if any transaction fails;
/// otherwise it will try to finish all transactions.
pub async fn batch_replay(
    tx_digests: impl Iterator<Item = TransactionDigest>,
    num_tasks: u64,
    rpc_url: String,
    expensive_safety_check_config: ExpensiveSafetyCheckConfig,
    use_authority: bool,
    terminate_early: bool,
    persist_path: Option<PathBuf>,
) {
    let provider = Arc::new(TransactionDigestProvider::new(tx_digests));
    let cancel = tokio_util::sync::CancellationToken::new();
    let mut tasks = vec![];
    let cur_time = Instant::now();
    for _ in 0..num_tasks {
        let provider = provider.clone();
        let expensive_safety_check_config = expensive_safety_check_config.clone();
        let rpc_url_ref = rpc_url.as_ref();
        let cancel = cancel.clone();
        let persist_path_ref = persist_path.as_ref();
        tasks.push(run_task(
            provider,
            rpc_url_ref,
            expensive_safety_check_config,
            use_authority,
            terminate_early,
            cancel,
            persist_path_ref,
        ));
    }
    let all_failed_transactions: Vec<_> = join_all(tasks).await.into_iter().flatten().collect();
    info!(
        "Finished replaying {} transactions, took {:?}",
        provider.get_executed_count(),
        cur_time.elapsed()
    );
    if all_failed_transactions.is_empty() {
        info!("All transactions passed");
    } else {
        error!("Some transactions failed: {:?}", all_failed_transactions);
    }
}

struct TransactionDigestProvider {
    digests: Mutex<VecDeque<TransactionDigest>>,
    total_count: usize,
    executed_count: AtomicUsize,
}

impl TransactionDigestProvider {
    pub fn new(digests: impl Iterator<Item = TransactionDigest>) -> Self {
        let digests: VecDeque<_> = digests.collect();
        let total_count = digests.len();
        Self {
            digests: Mutex::new(digests),
            total_count,
            executed_count: AtomicUsize::new(0),
        }
    }

    pub fn get_total_count(&self) -> usize {
        self.total_count
    }

    pub fn get_executed_count(&self) -> usize {
        self.executed_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Returns the index and digest of the next transaction, if any.
    pub fn next_digest(&self) -> Option<(usize, TransactionDigest)> {
        let next_digest = self.digests.lock().pop_front();
        next_digest.map(|digest| {
            let executed_count = self
                .executed_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            (executed_count + 1, digest)
        })
    }
}

async fn run_task(
    tx_digest_provider: Arc<TransactionDigestProvider>,
    http_url: &str,
    expensive_safety_check_config: ExpensiveSafetyCheckConfig,
    use_authority: bool,
    terminate_early: bool,
    cancel: tokio_util::sync::CancellationToken,
    persist_path: Option<&PathBuf>,
) -> Vec<ReplayEngineError> {
    let total_count = tx_digest_provider.get_total_count();
    let mut failed_transactions = vec![];
    let mut executor = LocalExec::new_from_fn_url(http_url).await.unwrap();
    while let Some((index, digest)) = tx_digest_provider.next_digest() {
        if cancel.is_cancelled() {
            break;
        }
        info!(
            "[{}/{}] Replaying transaction {:?}...",
            index, total_count, digest
        );
        let sandbox_persist_path = persist_path.map(|path| path.join(format!("{}.json", digest,)));
        if let Some(p) = sandbox_persist_path.as_ref() {
            if p.exists() {
                info!(
                    "Skipping transaction {:?} as it has been replayed before",
                    digest
                );
                continue;
            }
        }
        let async_func = execute_transaction(
            &mut executor,
            &digest,
            expensive_safety_check_config.clone(),
            use_authority,
        )
        .fuse();
        let result = tokio::select! {
            result = async_func => result,
            _ = cancel.cancelled() => {
                break;
            }
        };
        match result {
            Err(err) => {
                error!("Replaying transaction {:?} failed: {:?}", digest, err);
                failed_transactions.push(err.clone());
                if terminate_early {
                    cancel.cancel();
                    break;
                }
            }
            Ok(sandbox_state) => {
                info!("Replaying transaction {:?} succeeded", digest);
                if let Some(p) = sandbox_persist_path {
                    let out = serde_json::to_string(&sandbox_state).unwrap();
                    std::fs::write(p, out).unwrap();
                }
            }
        }
    }
    failed_transactions
}

async fn execute_transaction(
    executor: &mut LocalExec,
    digest: &TransactionDigest,
    expensive_safety_check_config: ExpensiveSafetyCheckConfig,
    use_authority: bool,
) -> Result<ExecutionSandboxState, ReplayEngineError> {
    *executor = loop {
        match executor.clone().reset_for_new_execution_with_client().await {
            Ok(executor) => break executor,
            Err(err) => {
                error!("Failed to reset executor: {:?}. Retrying in 3s", err);
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    };
    let sandbox_state = loop {
        let result = executor
            .execute_transaction(
                digest,
                expensive_safety_check_config.clone(),
                use_authority,
                None,
                None,
                None,
                None,
            )
            .await;
        match result {
            Ok(sandbox_state) => break sandbox_state,
            err @ Err(ReplayEngineError::TransactionNotSupported { .. }) => {
                return err;
            }
            Err(err) => {
                error!("Failed to execute transaction: {:?}. Retrying in 3s", err);
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    };
    sandbox_state.check_effects()?;
    Ok(sandbox_state)
}
