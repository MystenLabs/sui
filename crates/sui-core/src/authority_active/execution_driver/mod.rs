// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, sync::Arc};
use sui_types::{base_types::TransactionDigest, error::SuiResult, messages::VerifiedCertificate};
use tracing::{debug, info};

use crate::authority::AuthorityState;
use crate::authority_client::AuthorityAPI;

use futures::{stream, StreamExt};

use super::ActiveAuthority;

use tap::TapFallible;

#[cfg(test)]
pub(crate) mod tests;

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

/// Reads all pending transactions as a block and executes them.
/// Returns whether all pending transactions succeeded.
async fn execute_pending<A>(active_authority: Arc<ActiveAuthority<A>>) -> SuiResult<bool>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // Get the pending transactions
    let pending_transactions = active_authority.state.database.get_pending_digests()?;

    // Before executing de-duplicate the list of pending trasnactions
    let mut seen = HashSet::new();
    let mut indexes_to_delete = Vec::new();
    let pending_transactions: Vec<_> = pending_transactions
        .into_iter()
        .filter(|(idx, digest)| {
            if seen.contains(digest) {
                indexes_to_delete.push(*idx);
                false
            } else {
                seen.insert(*digest);
                true
            }
        })
        .collect();
    active_authority
        .state
        .database
        .remove_pending_digests(indexes_to_delete)?;

    // Send them for execution
    let epoch = active_authority.state.committee.load().epoch;
    let sync_handle = active_authority.clone().node_sync_handle();
    let executed: Vec<_> = sync_handle
        // map to extract digest
        .handle_execution_request(
            epoch,
            pending_transactions.iter().map(|(_, digest)| *digest),
        )
        .await?
        // zip results back together with seq
        .zip(stream::iter(pending_transactions.iter()))
        // filter out errors
        .filter_map(|(result, (seq, digest))| async move {
            result
                .tap_err(|e| info!(?seq, ?digest, "certificate execution failed: {}", e))
                .tap_ok(|_| debug!(?seq, ?digest, "certificate execution complete"))
                .ok()
                .map(|_| seq)
        })
        .collect()
        .await;

    let pending_count = pending_transactions.len();
    let executed_count = executed.len();
    debug!(?pending_count, ?executed_count, "execute_pending completed");

    // Now update the pending store.
    active_authority
        .state
        .database
        .remove_pending_digests(executed)?;

    Ok(pending_count == executed_count)
}
