// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use sui_types::{base_types::TransactionDigest, error::SuiResult, messages::CertifiedTransaction};
use tracing::{debug, info};

use crate::authority::AuthorityStore;
use crate::authority_client::AuthorityAPI;

use futures::{stream, StreamExt};

use super::ActiveAuthority;

#[cfg(test)]
pub(crate) mod tests;

pub trait PendCertificateForExecution {
    fn add_pending_certificates(
        &self,
        certs: Vec<(TransactionDigest, Option<CertifiedTransaction>)>,
    ) -> SuiResult<()>;
}

impl PendCertificateForExecution for Arc<AuthorityStore> {
    fn add_pending_certificates(
        &self,
        certs: Vec<(TransactionDigest, Option<CertifiedTransaction>)>,
    ) -> SuiResult<()> {
        self.as_ref().add_pending_certificates(certs)
    }
}

/// A no-op PendCertificateForExecution that we use for testing, when
/// we do not care about certificates actually being executed.
pub struct PendCertificateForExecutionNoop;
impl PendCertificateForExecution for PendCertificateForExecutionNoop {
    fn add_pending_certificates(
        &self,
        _certs: Vec<(TransactionDigest, Option<CertifiedTransaction>)>,
    ) -> SuiResult<()> {
        Ok(())
    }
}

/// When a notification that a new pending transaction is received we activate
/// processing the transaction in a loop.
pub async fn execution_process<A>(active_authority: &ActiveAuthority<A>)
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

        if let Err(err) = execute_pending(active_authority).await {
            tracing::error!("Error in pending execution subsystem: {err}");
            // The above should not return an error if the DB works, and we are connected to
            // the network. However if it does, we should backoff a little.
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    }
}

/// Reads all pending transactions as a block and executes them.
async fn execute_pending<A>(active_authority: &ActiveAuthority<A>) -> SuiResult<()>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // Get the pending transactions
    let pending_transactions = active_authority.state.database.get_pending_digests()?;

    let sync_handle = active_authority.node_sync_handle();

    // Send them for execution
    let executed = sync_handle
        // map to extract digest
        .handle_execution_request(pending_transactions.iter().map(|(_, digest)| *digest))
        // zip results back together with seq
        .zip(stream::iter(pending_transactions.iter()))
        // filter out errors
        .filter_map(|(result, (seq, _))| async move { result.ok().map(|_| seq) })
        .collect()
        .await;

    // Now update the pending store.
    active_authority
        .state
        .database
        .remove_pending_certificates(executed)?;

    Ok(())
}
