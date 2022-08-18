// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_active::ActiveAuthority;
use crate::authority_aggregator::{AuthorityAggregator, ReduceOutput};
use crate::authority_client::AuthorityAPI;
use async_trait::async_trait;
use multiaddr::Multiaddr;
use narwhal_crypto::traits::ToFromBytes;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use sui_network::tonic;
use sui_types::base_types::AuthorityName;
use sui_types::committee::{Committee, EpochId, StakeUnit};
use sui_types::crypto::{AuthorityPublicKeyBytes, AuthoritySignature};
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{
    AuthenticatedEpoch, CertifiedEpoch, EpochInfoDigest, EpochRequest, EpochResponse,
    SignedTransaction, Transaction,
};
use sui_types::sui_system_state::SuiSystemState;
use tracing::{debug, error, info, warn};
use typed_store::Map;

#[async_trait]
pub trait Reconfigurable {
    fn needs_network_recreation() -> bool;

    fn recreate(channel: tonic::transport::Channel) -> Self;
}

// TODO: Move these constants to a control config.
const WAIT_BETWEEN_EPOCH_QUERY_RETRY: Duration = Duration::from_millis(300);
const QUORUM_TIMEOUT: Duration = Duration::from_secs(60);

impl<A> ActiveAuthority<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone + Reconfigurable,
{
    /// This function should be called by the active checkpoint process, when it finishes processing
    /// all transactions from the second to the least checkpoint of the epoch. It's called by a
    /// validator that belongs to the committee of the current epoch.
    pub async fn start_epoch_change(&self) -> SuiResult {
        let epoch = self.state.committee.load().epoch;
        info!(?epoch, "Starting epoch change");
        let checkpoints = self.state.checkpoints.as_ref().unwrap();
        assert!(
            checkpoints.lock().is_ready_to_start_epoch_change(),
            "start_epoch_change called at the wrong checkpoint",
        );

        self.state.halt_validator();
        info!(?epoch, "Validator halted for epoch change");
        self.wait_for_validator_batch().await;
        info!(?epoch, "Epoch change started");
        Ok(())
    }

    /// This function should be called by the active checkpoint process, when it finishes processing
    /// all transactions from the last checkpoint of the epoch. This function needs to be called by
    /// a validator that belongs to the committee of the next epoch.
    pub async fn finish_epoch_change(&self) -> SuiResult {
        let epoch = self.state.committee.load().epoch;
        info!(?epoch, "Finishing epoch change");
        let checkpoints = self.state.checkpoints.as_ref().unwrap();
        {
            let mut checkpoints = checkpoints.lock();
            assert!(
                checkpoints.is_ready_to_finish_epoch_change(),
                "finish_epoch_change called at the wrong checkpoint",
            );

            for (tx_digest, _) in checkpoints.tables.extra_transactions.iter() {
                warn!(?epoch, tx_digest=?tx_digest.transaction, "Reverting local transaction effects");
                self.state
                    .database
                    .revert_state_update(&tx_digest.transaction)?;
            }

            // Delete any extra certificates now unprocessed.
            checkpoints.tables.extra_transactions.clear()?;

            self.state.database.remove_all_pending_certificates()?;
        }
        let next_checkpoint = checkpoints.lock().next_checkpoint();

        let sui_system_state = self.state.get_sui_system_state_object().await?;
        let next_epoch = epoch + 1;
        let next_epoch_validators = &sui_system_state.validators.next_epoch_validators;
        let votes = next_epoch_validators
            .iter()
            .map(|metadata| {
                (
                    AuthorityPublicKeyBytes::from_bytes(metadata.pubkey_bytes.as_ref())
                        .expect("Validity of public key bytes should be verified on-chain"),
                    metadata.next_epoch_stake + metadata.next_epoch_delegation,
                )
            })
            .collect();
        let new_committee = Committee::new(next_epoch, votes)?;
        debug!(
            ?epoch,
            "New committee for the next epoch: {}", new_committee
        );
        let old_committee = self.state.clone_committee();
        self.state
            .sign_new_epoch_and_update_committee(new_committee.clone(), next_checkpoint)?;

        loop {
            let err = match self
                .wait_for_epoch_cert(next_epoch, &old_committee, QUORUM_TIMEOUT)
                .await
            {
                Ok(cert) => {
                    info!(epoch=?next_epoch, "Epoch Certificate Formed");
                    debug!("Epoch Certificate: {:?}", cert);
                    match self.state.promote_signed_epoch_to_cert(cert) {
                        Ok(()) => {
                            break;
                        }
                        Err(err) => err,
                    }
                }
                Err(err) => err,
            };
            debug!(
                ?epoch,
                "Failed to obtain certificate for the next epoch: {:?}", err
            );
            // TODO: Instead of actively waiting in a loop, we could instead use a notification
            // pattern.
            tokio::time::sleep(WAIT_BETWEEN_EPOCH_QUERY_RETRY).await;
        }

        // Reconnect the network if we have an type of AuthorityClient that has a network.
        let new_clients = if A::needs_network_recreation() {
            self.recreate_network(sui_system_state)?
        } else {
            self.net.load().clone_inner_clients()
        };
        // Replace the clients in the authority aggregator with new clients.
        let new_net = Arc::new(AuthorityAggregator::new(
            new_committee,
            self.state.epoch_store().clone(),
            new_clients,
            self.net.load().metrics.clone(),
            self.net.load().safe_client_metrics.clone(),
        ));
        self.net.store(new_net);

        // TODO: Update all committee in all components safely,
        // potentially restart narwhal committee/consensus adapter,
        // all active processes, maybe batch service.
        // We should also reduce the amount of committee passed around.

        let advance_epoch_tx = SignedTransaction::new_change_epoch(
            next_epoch,
            0, // TODO: fill in storage_charge
            0, // TODO: fill in computation_charge
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
                .process_transaction(Transaction::from_signed(advance_epoch_tx.clone()))
                .await
            {
                Ok(certificate) => match self.state.handle_certificate(certificate).await {
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
            tokio::time::sleep(WAIT_BETWEEN_EPOCH_QUERY_RETRY).await;
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

        let mut net_config = mysten_network::config::Config::new();
        net_config.connect_timeout = Some(Duration::from_secs(5));
        net_config.request_timeout = Some(Duration::from_secs(5));
        net_config.http2_keepalive_interval = Some(Duration::from_secs(5));

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
            let client: A = A::recreate(channel);
            debug!(
                "New network client created for {} at {:?}",
                public_key_bytes, address
            );
            new_clients.insert(public_key_bytes, client);
        }
        Ok(new_clients)
    }

    async fn wait_for_epoch_cert(
        &self,
        epoch_id: EpochId,
        old_committee: &Committee,
        timeout_until_quorum: Duration,
    ) -> SuiResult<CertifiedEpoch> {
        #[derive(Default)]
        struct Signatures {
            total_stake: StakeUnit,
            sigs: Vec<(AuthorityName, AuthoritySignature)>,
        }
        #[derive(Default)]
        struct Summaries {
            bad_weight: StakeUnit,
            signed: BTreeMap<EpochInfoDigest, Signatures>,
            cert: Option<CertifiedEpoch>,
            errors: Vec<(AuthorityName, SuiError)>,
        }
        let initial_state = Summaries::default();
        let net = self.net.load();
        let threshold = net.committee.quorum_threshold();
        let validity = net.committee.validity_threshold();
        let final_state = net
            .quorum_map_then_reduce_with_timeout(
                initial_state,
                |_name, client| {
                    Box::pin(async move {
                        client
                            .handle_epoch(EpochRequest { epoch_id: Some(epoch_id) })
                            .await
                    })
                },
                |mut state, name, weight, result| {
                    Box::pin(async move {
                        if let Ok(EpochResponse {
                                      epoch_info: Some(epoch_info)
                                  }) = result
                        {
                            match epoch_info {
                                AuthenticatedEpoch::Signed(s) => {
                                    let entry = state.signed.entry(s.epoch_info.digest()).or_default();
                                    entry.total_stake += weight;
                                    entry.sigs.push((name, s.auth_signature.signature));
                                    if entry.total_stake >= threshold {
                                        let maybe_cert = CertifiedEpoch::new(
                                            &s.epoch_info,
                                            entry.sigs.clone(),
                                            old_committee
                                        );
                                        match maybe_cert {
                                            Ok(cert) => {
                                                state.cert = Some(cert);
                                                return Ok(ReduceOutput::End(state));
                                            },
                                            Err(err) => {
                                                error!("Unexpected error when creating epoch cert: {:?}", err);
                                                state.errors.push((name, err));
                                            }
                                        }
                                    }
                                }
                                AuthenticatedEpoch::Certified(c) => {
                                    state.cert = Some(c);
                                    return Ok(ReduceOutput::End(state));
                                }
                                AuthenticatedEpoch::Genesis(_) => {
                                    unreachable!();
                                }
                            }
                        } else {
                            state.bad_weight += weight;

                            // Add to the list of errors.
                            match result {
                                Err(err) => state.errors.push((name, err)),
                                Ok(_) => state.errors.push((
                                    name,
                                    SuiError::from("Epoch info not yet available"),
                                )),
                            }

                            // Return all errors if a quorum is not possible.
                            if state.bad_weight > validity {
                                return Err(SuiError::TooManyIncorrectAuthorities {
                                    errors: state.errors,
                                    action: "wait_for_epoch_cert",
                                });
                            }
                        }

                        Ok(ReduceOutput::Continue(state))
                    })
                },
                // A long timeout before we hear back from a quorum
                timeout_until_quorum,
            )
            .await?;
        if let Some(cert) = final_state.cert {
            Ok(cert)
        } else {
            Err(SuiError::TooManyIncorrectAuthorities {
                errors: final_state.errors,
                action: "wait_for_epoch_cert",
            })
        }
    }

    /// Check that all transactions that have been sequenced and are about to be committed get
    /// committed. Also make sure that all the transactions that have been committed have made
    /// into a batch. This ensures that they will all made to the next checkpoint proposal.
    /// TODO: We need to ensure that this does not halt forever and is more efficient.
    /// https://github.com/MystenLabs/sui/issues/3915
    async fn wait_for_validator_batch(&self) {
        while !self.state.batch_notifier.ticket_drained_til(
            self.state
                .checkpoints
                .as_ref()
                .unwrap()
                .lock()
                .next_transaction_sequence_expected(),
        ) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
