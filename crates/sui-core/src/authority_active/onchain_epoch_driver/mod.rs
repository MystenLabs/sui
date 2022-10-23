// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use crate::authority_client::AuthorityAPI;
use crate::consensus_adapter::ConsensusAdapter;
use crate::safe_client::SafeClient;
use std::sync::Arc;
use std::time::Duration;
use sui_types::committee::StakeUnit;
use sui_types::messages::{CertifiedTransaction, SignedTransaction};
use tokio::time::sleep;
use tracing::{debug, error};

// Change epoch every hour.
const EPOCH_CHANGE_INTERVAL: Duration = Duration::from_secs(60 * 60);
const WAIT_FOR_CERT_SLEEP_INTERVAL: Duration = Duration::from_secs(5);

pub async fn onchain_epoch_driver_process<A>(
    self_client: SafeClient<A>,
    state: Arc<AuthorityState>,
    consensus_adapter: Arc<ConsensusAdapter>,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    loop {
        sleep(EPOCH_CHANGE_INTERVAL).await;
        submit_new_change_epoch_tx(&state, &consensus_adapter).await;

        loop {
            sleep(WAIT_FOR_CERT_SLEEP_INTERVAL).await;
            if let Some(cert) = check_epoch_tx_cert(&state) {
                if let Err(err) = self_client.handle_certificate(cert).await {
                    error!("Executing epoch change transaction failed: {:?}", err);
                }
                break;
            }
        }
    }
}

pub fn check_epoch_tx_cert(state: &Arc<AuthorityState>) -> Option<CertifiedTransaction> {
    let committee = state.committee.load();
    let mut onchain_epoch = state.onchain_epoch.lock();
    let total_stake: StakeUnit = onchain_epoch
        .epoch_change_transactions
        .iter()
        .map(|(name, _)| committee.weight(name))
        .sum();
    if total_stake >= committee.quorum_threshold() {
        let sigs = onchain_epoch
            .epoch_change_transactions
            .iter()
            .map(|(name, signed_tx)| (*name, signed_tx.auth_sign_info.signature.clone()))
            .collect();
        let cert = CertifiedTransaction::new_with_signatures(
            onchain_epoch
                .epoch_change_transactions
                .values()
                .next()
                .unwrap()
                .clone()
                .to_transaction(),
            sigs,
            &committee,
        )
        .expect("Aggregate verified signatures cannot fail");
        debug!(
            cur_epoch=?onchain_epoch.next_epoch,
            "Epoch change transaction certificate formed",
        );

        onchain_epoch.next_epoch += 1;
        onchain_epoch.epoch_change_transactions.clear();

        Some(cert)
    } else {
        None
    }
}

pub async fn submit_new_change_epoch_tx(
    state: &Arc<AuthorityState>,
    consensus_adapter: &Arc<ConsensusAdapter>,
) {
    let cur_epoch = state.onchain_epoch.lock().next_epoch;
    let change_epoch_tx =
        SignedTransaction::new_change_epoch(cur_epoch, 0, 0, 0, state.name, &*state.secret);
    if let Err(err) = consensus_adapter
        .submit_signed_epoch_change_tx(&state.name, &change_epoch_tx)
        .await
    {
        error!(
            ?cur_epoch,
            "Error submitting signed epoch change transaction: {:?}", err
        );
    } else {
        debug!(
            ?cur_epoch,
            "Signed epoch change transaction successfully submitted to consensus",
        );
    }
}
