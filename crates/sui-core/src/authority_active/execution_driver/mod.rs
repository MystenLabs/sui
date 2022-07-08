// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use sui_types::{base_types::TransactionDigest, error::SuiResult, messages::CertifiedTransaction};
use tracing::debug;
use typed_store::Map;

use crate::authority::AuthorityStore;
use crate::authority_client::AuthorityAPI;

use super::{gossip::LocalCertificateHandler, ActiveAuthority};

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
    debug!("Start pending certificates execution.");

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
    let _committee = active_authority.state.committee.load().clone();
    let net = active_authority.net.load().clone();

    // Get the pending transactions
    let pending_transactions = active_authority.state.database.get_pending_certificates()?;

    // Get all the actual certificates mapping to these pending transactions
    let certs = active_authority
        .state
        .database
        .certificates
        .multi_get(pending_transactions.iter().map(|(_, d)| *d))?;

    // Zip seq, digest with certs. Note the cert must exist in the DB
    let cert_seq: Vec<_> = pending_transactions
        .iter()
        .zip(certs.iter())
        .map(|((i, d), c)| (i, d, c.as_ref().expect("certificate must exist")))
        .collect();

    let local_handler = LocalCertificateHandler {
        state: active_authority.state.clone(),
    };

    // TODO: implement properly efficient execution for the block of transactions.
    let mut executed = vec![];
    for (i, d, c) in cert_seq {
        // Only execute if not already executed.
        if active_authority.state.database.effects_exists(d)? {
            executed.push(*i);
            continue;
        }

        debug!(digest=?d, "Pending execution for certificate.");

        // Sync and Execute with local authority state
        net.sync_certificate_to_authority_with_timeout_inner(
            c.clone(),
            active_authority.state.name,
            &local_handler,
            tokio::time::Duration::from_secs(10),
            10,
        )
        .await?;

        // Remove from the execution list
        executed.push(*i);
    }

    // Now update the pending store.
    active_authority
        .state
        .database
        .remove_pending_certificates(executed)?;

    Ok(())
}
