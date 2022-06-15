// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tracing::{debug};
use typed_store::Map;

use crate::authority_client::AuthorityAPI;

use super::{gossip::LocalConfirmationTransactionHandler, ActiveAuthority};

#[cfg(test)]
pub(crate) mod tests;

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

        // Get a copy of the committee:
        let _committee = active_authority.state.committee.load().clone();
        let _net = active_authority.net.load().clone();
        // TODO: check for committee change to signal epoch change and deal with it.

        // Get the pending transactions
        let pending_transactions = active_authority
            .state
            .database
            .get_pending_certificates()
            .expect("If an error occurs here we are dead");

        // Get all the actual certificates mapping to these pending transactions
        let certs = active_authority
            .state
            .database
            .certificates
            .multi_get(pending_transactions.iter().map(|(_, d)| *d))
            .expect("We cannot tolerate DB errors here.");

        // Zip seq, digest with certs. Note the cert must exist in the DB
        let cert_seq: Vec<_> = pending_transactions
            .iter()
            .zip(certs.iter())
            .map(|((i, d), c)| (i, d, c.as_ref().expect("certificate must exist")))
            .collect();

        let local_handler = LocalConfirmationTransactionHandler {
            state: active_authority.state.clone(),
        };

        let mut executed = vec![];
        for (i, d, c) in cert_seq {
            // Only execute if not already executed.
            if active_authority
                .state
                .database
                .effects_exists(d)
                .expect("DB should be ok.")
            {
                executed.push(*i);
                continue;
            }

            debug!(digest=?d, "Pending execution for certificate.");

            // Sync and Execute with local authority state
            _net.sync_certificate_to_authority_with_timeout_inner(
                sui_types::messages::ConfirmationTransaction::new(c.clone()),
                active_authority.state.name,
                &local_handler,
                tokio::time::Duration::from_secs(10),
                10,
            )
            .await
            .expect("Assume this work for now.");

            // Remove from the execution list
            executed.push(*i);
        }

        // Now update the pending store.
        active_authority
            .state
            .database
            .remove_pending_certificates(executed)
            .expect("DB should be ok");
    }
}
