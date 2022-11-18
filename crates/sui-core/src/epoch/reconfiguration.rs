// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::NetworkAuthorityClientMetrics;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sui_network::tonic;

#[async_trait]
pub trait Reconfigurable {
    fn needs_network_recreation() -> bool;

    fn recreate(
        channel: tonic::transport::Channel,
        metrics: Arc<NetworkAuthorityClientMetrics>,
    ) -> Self;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ReconfigCertStatus {
    AcceptAllCerts,
    RejectUserCerts,
    RejectAllCerts,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReconfigState {
    status: ReconfigCertStatus,
}

impl Default for ReconfigState {
    fn default() -> Self {
        Self {
            status: ReconfigCertStatus::AcceptAllCerts,
        }
    }
}

impl ReconfigState {
    pub fn close_user_certs(&mut self) {
        if matches!(self.status, ReconfigCertStatus::AcceptAllCerts) {
            self.status = ReconfigCertStatus::RejectUserCerts;
        }
    }

    pub fn is_reject_user_certs(&self) -> bool {
        matches!(self.status, ReconfigCertStatus::RejectUserCerts)
    }

    pub fn close_all_certs(&mut self) {
        self.status = ReconfigCertStatus::RejectAllCerts;
    }

    pub fn open_all_certs(&mut self) {
        self.status = ReconfigCertStatus::AcceptAllCerts;
    }

    pub fn should_accept_user_certs(&self) -> bool {
        matches!(self.status, ReconfigCertStatus::AcceptAllCerts)
    }

    pub fn should_accept_consensus_certs(&self) -> bool {
        !matches!(self.status, ReconfigCertStatus::RejectAllCerts)
    }
}

/*

use crate::authority_active::ActiveAuthority;
use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::{AuthorityAPI, NetworkAuthorityClientMetrics};
use async_trait::async_trait;
use fastcrypto::traits::ToFromBytes;
use itertools::MultiUnzip;
use multiaddr::Multiaddr;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;
use sui_network::{default_mysten_network_config, tonic};
use sui_types::base_types::AuthorityName;
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::error::SuiResult;
use sui_types::messages::VerifiedSignedTransaction;
use sui_types::sui_system_state::SuiSystemState;
use tracing::{debug, error, info, warn};
use typed_store::Map;

const WAIT_BETWEEN_QUORUM_QUERY_RETRY: Duration = Duration::from_millis(300);

impl<A> ActiveAuthority<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone + Reconfigurable,
{
    /// This function should be called by the active checkpoint process, when it finishes processing
    /// all transactions from the second to the least checkpoint of the epoch. It's called by a
    /// validator that belongs to the committee of the current epoch.
    pub async fn start_epoch_change(&self) -> SuiResult {
        let checkpoints = &self.state.checkpoints;
        assert!(
            checkpoints.lock().is_ready_to_start_epoch_change(),
            "start_epoch_change called at the wrong checkpoint",
        );
        let epoch = self.state.committee.load().epoch;
        info!(?epoch, "Starting epoch change");
        self.state.halt_validator();
        info!(?epoch, "Validator halted for epoch change");
        self.wait_for_validator_batch().await?;
        info!(?epoch, "Epoch change started");
        Ok(())
    }

    /// This function should be called by the active checkpoint process, when it finishes processing
    /// all transactions from the last checkpoint of the epoch. This function needs to be called by
    /// a validator that belongs to the committee of the next epoch.
    pub async fn finish_epoch_change(&self) -> SuiResult {
        let epoch = self.state.committee.load().epoch;
        info!(?epoch, "Finishing epoch change");
        let checkpoints = &self.state.checkpoints;
        let (storage_charge, computation_charge, storage_rebate) = {
            let mut checkpoints = checkpoints.lock();
            assert!(
                checkpoints.is_ready_to_finish_epoch_change(),
                "finish_epoch_change called at the wrong checkpoint",
            );

            let pending_tx: BTreeSet<_> = checkpoints
                .tables
                .extra_transactions
                .iter()
                .map(|(digest, _)| digest)
                .collect();
            let extra_tx =
                checkpoints.filter_already_checkpointed_transactions(pending_tx.iter())?;
            for tx_digest in extra_tx {
                warn!(?epoch, tx_digest=?tx_digest.transaction, "Reverting local transaction effects");
                self.state
                    .database
                    .revert_state_update(&tx_digest.transaction)?;
            }

            // Delete any extra certificates now unprocessed.
            checkpoints.tables.extra_transactions.clear()?;
            // This is either unnecessary if the whole epoch database will be dropped, or
            // in correct if the table can contain certificates from multiple epochs.
            // TODO: fix this during reconfiguration work.
            self.state.database.cleanup_pending_certificates()?;
            // TODO: also clean up self.node_sync_store for epoch - 1.

            let (storage_charges, computation_charges, storage_rebates): (
                Vec<u64>,
                Vec<u64>,
                Vec<u64>,
            ) = checkpoints
                .get_checkpoints_of_epoch(epoch)
                .iter()
                .map(|cp| {
                    (
                        cp.summary().gas_cost_summary.storage_cost,
                        cp.summary().gas_cost_summary.computation_cost,
                        cp.summary().gas_cost_summary.storage_rebate,
                    )
                })
                .multiunzip();
            (
                storage_charges.iter().sum(),
                computation_charges.iter().sum(),
                storage_rebates.iter().sum(),
            )
        };

        let sui_system_state = self.state.get_sui_system_state_object().await?;
        let next_epoch = epoch + 1;

        // Create new AuthorityEpochTables for epoch-specific data.
        self.state.database.reopen_epoch_db(next_epoch);

        let new_committee = sui_system_state.get_next_epoch_committee();
        debug!(
            ?epoch,
            "New committee for the next epoch: {}", new_committee
        );
        self.state.update_committee(new_committee.clone())?;

        // Reconnect the network if we have an type of AuthorityClient that has a network.
        let new_clients = if A::needs_network_recreation() {
            self.recreate_network(sui_system_state)?
        } else {
            self.net.load().clone_inner_clients()
        };
        // Replace the clients in the authority aggregator with new clients.
        let new_net = Arc::new(AuthorityAggregator::new(
            new_committee,
            self.state.committee_store().clone(),
            new_clients,
            self.net.load().metrics.clone(),
            self.net.load().safe_client_metrics.clone(),
            self.net.load().network_client_metrics.clone(),
        ));
        self.net.store(new_net);

        // TODO: Update all committee in all components safely,
        // potentially restart narwhal committee/consensus adapter,
        // all active processes, maybe batch service.
        // We should also reduce the amount of committee passed around.

        let advance_epoch_tx = VerifiedSignedTransaction::new_change_epoch(
            next_epoch,
            storage_charge,
            computation_charge,
            storage_rebate,
            self.state.name,
            &*self.state.secret,
        );
        debug!(
            ?epoch,
            "System transaction to advance epoch: {:?}", advance_epoch_tx
        );
        // Add the signed transaction to the store.
        self.state
            .set_transaction_lock(&[], advance_epoch_tx.clone())
            .await?;

        // Collect a certificate for this system transaction that changes epoch,
        // and execute it locally.
        loop {
            let err = match self
                .net
                .load()
                .process_transaction(advance_epoch_tx.clone().into_unsigned())
                .await
            {
                Ok(certificate) => match self.state.handle_certificate(&certificate).await {
                    Ok(_) => {
                        break;
                    }
                    Err(err) => err,
                },
                Err(err) => err,
            };
            debug!(
                ?epoch,
                "Error when processing advance epoch transaction: {:?}", err
            );
            tokio::time::sleep(WAIT_BETWEEN_QUORUM_QUERY_RETRY).await;
        }

        // Resume the validator to start accepting transactions for the new epoch.
        self.state.unhalt_validator();
        info!(?epoch, "Validator unhalted.");

        info!(
            "===== Epoch change finished. We are now at epoch {:?} =====",
            next_epoch
        );
        Ok(())
    }

    /// Recreates the network if the client is a type of client that has a network, and swap the new
    /// clients onto the authority aggregator with the new committee.
    pub fn recreate_network(
        &self,
        sui_system_state: SuiSystemState,
    ) -> SuiResult<BTreeMap<AuthorityName, A>> {
        let mut new_clients = BTreeMap::new();
        let next_epoch_validators = sui_system_state.validators.next_epoch_validators;
        let net_config = default_mysten_network_config();
        let cur_clients = self.net.load().authority_clients.clone();

        for validator in next_epoch_validators {
            let public_key_bytes = match AuthorityPublicKeyBytes::from_bytes(
                &validator.pubkey_bytes,
            ) {
                Err(err) => {
                    error!("Error parsing validator public key. Skip this validator in the committee: {:?}", err);
                    continue;
                }
                Ok(result) => result,
            };
            // TODO: We only recreate network connection if this is a new validator.
            // This is because creating a new network connection on the same address doesn't
            // work. We may want to look into this and see why it doesn't work.
            if let Some(existing_client) = cur_clients.get(&public_key_bytes) {
                // TODO: Since we rely purely on the public key to decide whether to recreate
                // the network, it means that validators won't be able to modify their network
                // information without also using a new public key.
                new_clients.insert(public_key_bytes, existing_client.authority_client().clone());
                debug!(
                    "Adding unchanged client to the new network: {}",
                    public_key_bytes
                );
                continue;
            }

            let address = match Multiaddr::try_from(validator.net_address) {
                Err(err) => {
                    error!("Error parsing validator network address. Skip this validator in the committee: {:?}", err);
                    continue;
                }
                Ok(result) => result,
            };

            let channel = match net_config.connect_lazy(&address) {
                Err(err) => {
                    error!("Error connecting to client {} with address {:?}. Skip this validator in the committee: {:?}", public_key_bytes, address, err);
                    continue;
                }
                Ok(result) => result,
            };
            let client: A = A::recreate(channel, self.network_metrics.clone());
            debug!(
                "New network client created for {} at {:?}",
                public_key_bytes, address
            );
            new_clients.insert(public_key_bytes, client);
        }
        Ok(new_clients)
    }

    /// Check that all transactions that have been sequenced and are about to be committed get
    /// committed. Also make sure that all the transactions that have been committed have made
    /// into a batch. This ensures that they will all made to the next checkpoint proposal.
    /// TODO: We need to ensure that this does not halt forever and is more efficient.
    /// https://github.com/MystenLabs/sui/issues/3915
    async fn wait_for_validator_batch(&self) -> SuiResult {
        let last_ticket = loop {
            match self.state.batch_notifier.ticket_drained() {
                Some(ticket) => break ticket,
                None => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        };
        let mut checkpoint_store = self.state.checkpoints.lock();
        let mut unbatched = self.state.database.transactions_in_seq_range(
            checkpoint_store.next_transaction_sequence_expected(),
            last_ticket,
        )?;
        let extra_tx_seqs: BTreeSet<_> = checkpoint_store
            .tables
            .extra_transactions
            .iter()
            .map(|(_, seq)| seq)
            .collect();
        unbatched.retain(|(seq, _)| !extra_tx_seqs.contains(seq));
        checkpoint_store.handle_internal_batch(last_ticket, &unbatched[..])?;
        Ok(())
    }
}
 */
