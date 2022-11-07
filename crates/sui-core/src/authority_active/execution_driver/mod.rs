// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_int_counter_with_registry, register_int_gauge_with_registry, IntCounter, IntGauge,
    Registry,
};
use std::{collections::HashSet, sync::Arc};
use sui_types::{base_types::TransactionDigest, error::SuiResult, messages::VerifiedCertificate};
use tracing::{debug, error, info};

use crate::authority::{AuthorityState, PendingDigest};
use crate::authority_client::AuthorityAPI;

use futures::{stream, StreamExt};

use super::ActiveAuthority;

use tap::TapFallible;

#[cfg(test)]
pub(crate) mod tests;

#[derive(Clone)]
pub struct ExecutionDriverMetrics {
    executed_transactions: IntCounter,
    pending_transactions: IntGauge,
}

impl ExecutionDriverMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            executed_transactions: register_int_counter_with_registry!(
                "execution_driver_executed_transactions",
                "Cumulative number of transaction executed by execution driver",
                registry,
            )
            .unwrap(),
            pending_transactions: register_int_gauge_with_registry!(
                "execution_driver_pending_transaction",
                "Number of current pending transactions for execution driver",
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}

pub trait PendCertificateForExecution {
    fn add_pending_certificates(
        &self,
        certs: Vec<(TransactionDigest, Option<VerifiedCertificate>)>,
    ) -> SuiResult<()>;
}

impl PendCertificateForExecution for &AuthorityState {
    fn add_pending_certificates(
        &self,
        certs: Vec<(TransactionDigest, Option<VerifiedCertificate>)>,
    ) -> SuiResult<()> {
        AuthorityState::add_pending_certificates(self, certs)
    }
}

/// A no-op PendCertificateForExecution that we use for testing, when
/// we do not care about certificates actually being executed.
pub struct PendCertificateForExecutionNoop;
impl PendCertificateForExecution for PendCertificateForExecutionNoop {
    fn add_pending_certificates(
        &self,
        _certs: Vec<(TransactionDigest, Option<VerifiedCertificate>)>,
    ) -> SuiResult<()> {
        Ok(())
    }
}

/// When a notification that a new pending transaction is received we activate
/// processing the transaction in a loop.
pub async fn execution_process<A>(active_authority: Arc<ActiveAuthority<A>>)
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    info!("Start pending certificates execution process.");

    // Loop whenever there is a signal that a new transactions is ready to process.
    loop {
        // NOTE: nothing terrible happens if we fire more often than there are
        //       transactions awaiting execution, or less often than once per transactions.
        //       However, we need to be sure that if there is an awaiting trasnactions we
        //       will eventually fire the notification and wake up here.
        active_authority.state.database.wait_for_new_pending().await;

        debug!("Pending certificate execution activated.");

        // Process any tx that failed to commit.
        if let Err(err) = active_authority.state.process_tx_recovery_log(None).await {
            tracing::error!("Error processing tx recovery log: {:?}", err);
        }

        match execute_pending(active_authority.clone()).await {
            Err(err) => {
                tracing::error!("Error in pending execution subsystem: {err}");
                // The above should not return an error if the DB works, and we are connected to
                // the network. However if it does, we should backoff a little.
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
            Ok(true) => {
                // TODO: Move all of these timings into a control.
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            Ok(false) => {
                // Some execution failed. Wait a bit before we retry.
                // TODO: We may want to make the retry delay per-transaction, instead of
                // applying to the entire loop.
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

type PendingVec = Vec<(u64, PendingDigest)>;

fn sort_and_partition_pending_certs(
    mut pending_transactions: PendingVec,
) -> (
    PendingVec, // sequenced
    PendingVec, // unsequenced
    Vec<u64>,   // duplicated indices, to be deleted
) {
    // sort sequenced digests before unsequenced so that the deduplication below favors
    // sequenced digests.
    pending_transactions.sort_by(|(idx_a, (is_seq_a, _)), (idx_b, (is_seq_b, _))| {
        match is_seq_b.cmp(is_seq_a) {
            // when both are sequenced or unsequenced, sort by idx.
            std::cmp::Ordering::Equal => idx_a.cmp(idx_b),
            // otherwise sort sequenced before unsequenced
            res => res,
        }
    });

    // Before executing de-duplicate the list of pending trasnactions
    let mut seen = HashSet::new();
    let mut indexes_to_delete = Vec::new();

    let (pending_sequenced, pending_transactions): (Vec<_>, Vec<_>) = pending_transactions
        .into_iter()
        .filter(|(idx, (_, digest))| {
            if seen.contains(digest) {
                indexes_to_delete.push(*idx);
                false
            } else {
                seen.insert(*digest);
                true
            }
        })
        .partition(|(_, (is_sequenced, _))| *is_sequenced);

    debug!(
        num_sequenced = ?pending_sequenced.len(),
        num_unsequenced = ?pending_transactions.len()
    );

    (pending_sequenced, pending_transactions, indexes_to_delete)
}

#[test]
fn test_sort_and_partition_pending_certs() {
    let tx1 = TransactionDigest::random();
    let tx2 = TransactionDigest::random();
    let tx3 = TransactionDigest::random();
    let tx4 = TransactionDigest::random();

    // partitioning works correctly.
    assert_eq!(
        sort_and_partition_pending_certs(vec![(0, (false, tx1)), (1, (true, tx2))]),
        (vec![(1, (true, tx2))], vec![(0, (false, tx1))], vec![],)
    );

    // if certs are duplicated, but some are sequenced, the sequenced certs take priority.
    assert_eq!(
        sort_and_partition_pending_certs(vec![(0, (false, tx1)), (1, (true, tx1))]),
        (vec![(1, (true, tx1))], vec![], vec![0],)
    );

    // sorting works correctly for both sequenced and unsequenced.
    assert_eq!(
        sort_and_partition_pending_certs(vec![
            (2, (false, tx3)),
            (0, (false, tx2)),
            (4, (true, tx4)),
            (1, (true, tx1))
        ]),
        (
            vec![(1, (true, tx1)), (4, (true, tx4))],
            vec![(0, (false, tx2)), (2, (false, tx3))],
            vec![],
        )
    );
}

/// Reads all pending transactions as a block and executes them.
/// Returns whether all pending transactions succeeded.
async fn execute_pending<A>(active_authority: Arc<ActiveAuthority<A>>) -> SuiResult<bool>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // Get the pending transactions
    let pending_transactions = active_authority.state.database.get_pending_digests()?;

    active_authority
        .execution_driver_metrics
        .pending_transactions
        .set(pending_transactions.len() as i64);

    let (pending_sequenced, pending_transactions, indexes_to_delete) =
        sort_and_partition_pending_certs(pending_transactions);

    active_authority
        .state
        .database
        .remove_pending_digests(indexes_to_delete)?;

    // Send them for execution
    let epoch = active_authority.state.committee.load().epoch;
    let sync_handle = active_authority.clone().node_sync_handle();

    // Execute certs that have a sequencing index associated with them serially.
    for (seq, (_, digest)) in pending_sequenced.iter() {
        let mut result_stream = sync_handle
            .handle_execution_request(epoch, std::iter::once(*digest))
            .await?;

        match result_stream.next().await.unwrap() {
            Ok(_) => {
                debug!(?seq, ?digest, "serial certificate execution complete");
                active_authority
                    .execution_driver_metrics
                    .executed_transactions
                    .inc();
                active_authority
                    .state
                    .database
                    .remove_pending_digests(vec![*seq])
                    .tap_err(|err| {
                        error!(?seq, ?digest, "pending digest deletion failed: {}", err)
                    })?;
            }
            Err(err) => {
                info!(
                    ?seq,
                    ?digest,
                    "serial certificate execution failed: {}",
                    err
                );
            }
        }
    }

    let executed: Vec<_> = sync_handle
        // map to extract digest
        .handle_execution_request(
            epoch,
            pending_transactions.iter().map(|(_, (_, digest))| *digest),
        )
        .await?
        // zip results back together with seq
        .zip(stream::iter(pending_transactions.iter()))
        // filter out errors
        .filter_map(|(result, (idx, tx_digest))| async move {
            result
                .tap_err(|e| info!(?idx, ?tx_digest, "certificate execution failed: {}", e))
                .tap_ok(|_| debug!(?idx, ?tx_digest, "certificate execution complete"))
                .ok()
                .map(|_| idx)
        })
        .collect()
        .await;

    let pending_count = pending_transactions.len();
    let executed_count = executed.len();
    debug!(?pending_count, ?executed_count, "execute_pending completed");

    active_authority
        .execution_driver_metrics
        .executed_transactions
        .inc_by(executed_count as u64);

    // Now update the pending store.
    active_authority
        .state
        .database
        .remove_pending_digests(executed)?;

    Ok(pending_count == executed_count)
}
