// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::consensus::LeaderSchedule;
use crate::metrics::PrimaryMetrics;
use config::{AuthorityIdentifier, ChainIdentifier, Committee};
use crypto::{RandomnessPartialSignature, RandomnessPrivateKey};
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::groups;
use fastcrypto::serde_helpers::ToFromByteArray;
use fastcrypto_tbls::tbls::ThresholdBls;
use fastcrypto_tbls::types::{PublicVssKey, ThresholdBls12381MinSig};
use fastcrypto_tbls::{dkg, nodes};
use mysten_metrics::metered_channel::{Receiver, Sender};
use mysten_metrics::spawn_logged_monitored_task;
use mysten_network::anemo_ext::NetworkExt;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use storage::RandomnessStore;
use sui_protocol_config::ProtocolConfig;
use tap::TapFallible;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::WatchStream;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};
use types::{
    Certificate, CertificateAPI, ConditionalBroadcastReceiver, HeaderAPI, PrimaryToPrimaryClient,
    RandomnessRound, Round, SendRandomnessPartialSignaturesRequest, SystemMessage,
};

type PkG = groups::bls12381::G2Element;
type EncG = groups::bls12381::G2Element;

#[cfg(test)]
#[path = "tests/state_handler_tests.rs"]
pub mod state_handler_tests;

/// Updates Narwhal system state based on certificates received from consensus.
pub struct StateHandler {
    authority_id: AuthorityIdentifier,

    /// Receives the ordered certificates from consensus.
    rx_committed_certificates: Receiver<(Round, Vec<Certificate>)>,
    /// Channel to signal committee changes.
    rx_shutdown: ConditionalBroadcastReceiver,
    /// Channel to signal when the round changes.
    rx_narwhal_round_updates: WatchStream<Round>,
    /// Channel to receive partial signatures for randomness generation.
    rx_randomness_partial_signatures: Receiver<(
        AuthorityIdentifier,
        RandomnessRound,
        Vec<RandomnessPartialSignature>,
    )>,
    /// A channel to update the committed rounds
    tx_committed_own_headers: Option<Sender<(Round, Vec<Round>)>>,

    /// If set, generates Narwhal system messages for random beacon
    /// DKG and randomness generation.
    randomness_state: Option<RandomnessState>,

    network: anemo::Network,
}

// Internal state for randomness DKG and generation.
//
// DKG protocol:
// 1. This node sends out a `Message` to all other nodes.
// 2. Once sufficient valid `Message`s are received from other nodes via consensus and processed,
//    this node sends out a `Confirmation` to all other nodes.
// 3. Once sufficient `Confirmation`s are received from other nodes via consensus and processed,
//    they are combined to form a public VSS key and local private key shares.
// 4. Randomness generation begins.
//
// Randomness generation:
// 1. Randomness round begins at zero.
// 2. All nodes send `RandomnessPartialSignature`s for their shares on the round number to the next
//    Bullshark leader. This is repeated each time the leader changes.
// 3. Once sufficient partial signatures are received to reconstruct the full signature, it is sent
//    through consensus.
// 4. Once the full signature is received via consensus (verified with public VSS key),
//    randomness round is incremented.
struct RandomnessState {
    store: RandomnessStore,
    metrics: Arc<PrimaryMetrics>,

    // A channel to send system messages to the proposer.
    tx_system_messages: Sender<SystemMessage>,

    // State for DKG.
    party: dkg::Party<PkG, EncG>,
    has_sent_confirmation: bool, // enables re-sending Confirmation after crash recovery
    vss_key_output: Arc<OnceLock<PublicVssKey>>,
    dkg_output: Option<dkg::Output<PkG, EncG>>,

    // State for randomness generation.
    authority_id: AuthorityIdentifier,
    leader_schedule: LeaderSchedule,
    network: anemo::Network,
    last_randomness_round_sent: Option<RandomnessRound>,
    last_narwhal_round_sent: Round,
    narhwal_round: Round,
    // Partial signatures are expensive to compute, cached in case we need to re-send.
    cached_sigs: Option<(RandomnessRound, Vec<RandomnessPartialSignature>)>,
    // Partial sig storage is keyed on (randomness round, authority ID).
    // No need to save these for crash recovery since they are re-sent every time the next
    // Bullshark leader changes.
    partial_sigs: BTreeMap<(RandomnessRound, AuthorityIdentifier), Vec<RandomnessPartialSignature>>,
    partial_sig_sender: Option<JoinHandle<()>>,
}

impl RandomnessState {
    // Returns None in case of invalid input or other failure to initialize DKG.
    // In this case, narwhal will continue to function normally and simpluy not run
    // the random beacon protocol during the current epoch.
    fn try_new(
        chain: &ChainIdentifier,
        protocol_config: &ProtocolConfig,
        committee: Committee,
        authority_id: AuthorityIdentifier,
        private_key: RandomnessPrivateKey,
        leader_schedule: LeaderSchedule,
        network: anemo::Network,
        // Writes the VSS public key to this lock once DKG completes.
        vss_key_output: Arc<OnceLock<PublicVssKey>>,
        tx_system_messages: Sender<SystemMessage>,
        store: RandomnessStore,
        metrics: Arc<PrimaryMetrics>,
    ) -> Option<Self> {
        if !protocol_config.random_beacon() {
            info!("random beacon: disabled");
            return None;
        }

        let info = committee.randomness_dkg_info();
        if tracing::enabled!(tracing::Level::DEBUG) {
            // Log first few entries in DKG info for debugging.
            for (id, pk, stake) in info.iter().filter(|(id, _, _)| id.0 < 3) {
                let pk_bytes = pk.as_element().to_byte_array();
                debug!("random beacon: DKG info: id={id}, stake={stake}, pk={pk_bytes:x?}");
            }
        }
        let nodes = info
            .iter()
            .map(|(id, pk, stake)| nodes::Node::<EncG> {
                id: id.0,
                pk: pk.clone(),
                weight: *stake as u16,
            })
            .collect();
        let nodes = match nodes::Nodes::new(nodes) {
            Ok(nodes) => nodes,
            Err(err) => {
                error!("random beacon: error while initializing Nodes: {err:?}");
                return None;
            }
        };
        let (nodes, t) = nodes.reduce(
            committee
                .validity_threshold()
                .try_into()
                .expect("validity threshold should fit in u16"),
            protocol_config.random_beacon_reduction_allowed_delta(),
        );
        let total_weight = nodes.total_weight();
        let num_nodes = nodes.num_nodes();
        let prefix_str = format!(
            "dkg {} {}",
            Hex::encode(chain.as_bytes()),
            committee.epoch()
        );
        let party = match dkg::Party::<PkG, EncG>::new(
            private_key,
            nodes,
            t.into(),
            fastcrypto_tbls::random_oracle::RandomOracle::new(prefix_str.as_str()),
            &mut rand::thread_rng(),
        ) {
            Ok(party) => party,
            Err(err) => {
                error!("random beacon: error while initializing Party: {err:?}");
                return None;
            }
        };
        info!(
            "random beacon: state initialized with authority_id={authority_id}, total_weight={total_weight}, t={t}, num_nodes={num_nodes}, oracle initial_prefix={prefix_str:?}",
        );

        // Load existing data from store.
        let dkg_output = store.dkg_output();
        if let Some(dkg_output) = &dkg_output {
            info!(
                "random beacon: loaded existing DKG output for epoch {}",
                committee.epoch()
            );
            metrics
                .state_handler_random_beacon_dkg_num_shares
                .set(dkg_output.shares.as_ref().map_or(0, |shares| shares.len()) as i64);
            if let Err(e) = vss_key_output.set(dkg_output.vss_pk.clone()) {
                error!("random beacon: unable to write VSS key to output during startup: {e:?}")
            }
        } else {
            info!(
                "random beacon: no existing DKG output found for epoch {}",
                committee.epoch()
            );
        }
        metrics
            .state_handler_current_randomness_round
            .set(store.randomness_round().0 as i64);

        Some(Self {
            store,
            metrics,
            tx_system_messages,
            party,
            has_sent_confirmation: false,
            vss_key_output,
            dkg_output,
            authority_id,
            leader_schedule,
            network,
            last_randomness_round_sent: None,
            last_narwhal_round_sent: 0,
            narhwal_round: 0,
            cached_sigs: None,
            partial_sigs: BTreeMap::new(),
            partial_sig_sender: None,
        })
    }

    fn set_dkg_output(&mut self, output: dkg::Output<PkG, EncG>) {
        self.metrics
            .state_handler_random_beacon_dkg_num_shares
            .set(output.shares.as_ref().map_or(0, |shares| shares.len()) as i64);
        if let Err(e) = self.vss_key_output.set(output.vss_pk.clone()) {
            error!("random beacon: unable to write VSS key to output: {e:?}")
        }
        self.store.set_dkg_output(&output);
        self.dkg_output = Some(output);
    }

    async fn start_dkg(&self) {
        let msg = self.party.create_message(&mut rand::thread_rng());
        info!(
            "random beacon: sending DKG Message with sender={}, vss_pk.degree={}, encrypted_shares.len()={}",
            msg.sender,
            msg.vss_pk.degree(),
            msg.encrypted_shares.len(),
        );
        let _ = self
            .tx_system_messages
            .send(SystemMessage::DkgMessage(
                bcs::to_bytes(&msg).expect("message serialization should not fail"),
            ))
            .await;
    }

    fn add_message(&mut self, msg: dkg::Message<PkG, EncG>) {
        if self.store.has_used_messages() {
            // We've already sent a `Confirmation`, so we can't add any more messages.
            return;
        }
        match self.party.process_message(msg, &mut rand::thread_rng()) {
            Ok(processed) => self
                .store
                .add_processed_message(processed.message.sender, processed),
            Err(err) => {
                debug!("random beacon: error while processing DKG Message: {err:?}");
            }
        }
    }

    fn add_confirmation(&mut self, conf: dkg::Confirmation<EncG>) {
        if !self.store.has_used_messages() {
            // We should never see a `Confirmation` before we've sent our `Message` because
            // DKG messages are processed in consensus order.
            return;
        }
        if self.dkg_output.is_some() {
            // Once we have completed DKG, no more `Confirmation`s are needed.
            return;
        }
        self.store.add_confirmation(conf.sender, conf)
    }

    // Generates the next SystemMessage needed to advance the random beacon DKG protocol, if
    // possible, and sends it to the proposer.
    async fn advance_dkg(&mut self) {
        // Once we have enough ProcessedMessages, send a Confirmation.
        if !self.store.has_used_messages() || !self.has_sent_confirmation {
            match self.party.merge(&self.store.processed_messages()) {
                Ok((conf, used_msgs)) => {
                    info!(
                        "random beacon: sending DKG Confirmation with {} complaints",
                        conf.complaints.len()
                    );
                    self.store.set_used_messages(used_msgs);
                    self.has_sent_confirmation = true;
                    let _ = self
                        .tx_system_messages
                        .send(SystemMessage::DkgConfirmation(
                            bcs::to_bytes(&conf)
                                .expect("confirmation serialization should not fail"),
                        ))
                        .await;
                }
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => (), // wait for more input
                Err(e) => debug!("random beacon: error while merging DKG Messages: {e:?}"),
            }
        }

        // Once we have enough Confirmations, process them and update shares.
        if self.dkg_output.is_none() && self.store.has_used_messages() {
            match self.party.complete(
                self.store.used_messages().as_ref().expect("checked above"),
                &self.store.confirmations(),
                self.party.t() * 2 - 1, // t==f+1, we want 2f+1
                &mut rand::thread_rng(),
            ) {
                Ok(output) => {
                    let num_shares = output.shares.as_ref().map_or(0, |shares| shares.len());
                    self.set_dkg_output(output);
                    info!("random beacon: DKG complete with {num_shares} shares for this node");
                }
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => (), // wait for more input
                Err(e) => error!("random beacon: error while processing DKG Confirmations: {e:?}"),
            }
            // Begin randomness generation.
            if self.dkg_output.is_some() {
                info!("random beacon: start randomness generation");
                self.send_partial_signatures().await;
            }
        }
    }

    async fn update_narwhal_round(&mut self, round: Round) {
        self.narhwal_round = round;
        // Re-send partial signatures to new leader, in case the last one failed.
        if self.dkg_output.is_some() && (self.last_narwhal_round_sent <= round) {
            self.send_partial_signatures().await;
        }
    }

    async fn update_randomness_round(&mut self, round: RandomnessRound) {
        if round <= self.store.randomness_round() {
            // Don't go backwards.
            return;
        }
        debug!("random beacon: updating local randomness round to {round:?}");
        self.metrics
            .state_handler_current_randomness_round
            .set(round.0 as i64);
        self.store.set_randomness_round(round);
        self.partial_sigs.retain(|&(r, _), _| r >= round);
        self.send_partial_signatures().await;
    }

    async fn send_partial_signatures(&mut self) {
        let randomness_round = self.store.randomness_round();
        debug!("random beacon: sending partial signatures for round {randomness_round}",);

        if self.cached_sigs.is_none() || self.cached_sigs.as_ref().unwrap().0 != randomness_round {
            let shares = {
                let Some(dkg_output) = &self.dkg_output else {
                    error!("random beacon: called send_partial_signatures before DKG completed");
                    return;
                };
                match &dkg_output.shares {
                    Some(shares) => shares,
                    None => return, // can't participate in randomness generation without shares
                }
            };
            self.cached_sigs = Some((
                randomness_round,
                ThresholdBls12381MinSig::partial_sign_batch(
                    shares.iter(),
                    &randomness_round.signature_message(),
                ),
            ));
        }
        let sigs = self.cached_sigs.as_ref().unwrap().1.clone();

        // To compute next leader round, add two to even round, and one to odd round.
        let next_leader_narwhal_round = (self.narhwal_round + 2) & !1;
        self.last_narwhal_round_sent = next_leader_narwhal_round;

        let leader = self.leader_schedule.leader(next_leader_narwhal_round);
        if self.authority_id == leader.id() {
            // We're the next leader, no need to send an RPC.
            self.receive_partial_signatures(self.authority_id, randomness_round, sigs)
                .await;
            return;
        }

        let peer_id = anemo::PeerId(leader.network_key().0.to_bytes());
        let peer = self.network.waiting_peer(peer_id);
        let mut client = PrimaryToPrimaryClient::new(peer);
        const SEND_PARTIAL_SIGNATURES_TIMEOUT: Duration = Duration::from_secs(10);
        let request = anemo::Request::new(SendRandomnessPartialSignaturesRequest {
            round: randomness_round,
            sigs,
        })
        .with_timeout(SEND_PARTIAL_SIGNATURES_TIMEOUT);

        if let Some(task) = &self.partial_sig_sender {
            // Cancel previous partial signature transmission if it's not yet complete.
            task.abort();
        }
        self.partial_sig_sender = Some(spawn_logged_monitored_task!(
            async move {
                let resp = client.send_randomness_partial_signatures(request).await;
                if let Err(e) = resp {
                    info!(
                        "random beacon: error sending partial signatures to leader {leader:?}: {e:?}"
                    );
                }
            },
            "RandomnessSendPartialSignatures"
        ));
    }

    async fn receive_partial_signatures(
        &mut self,
        authority_id: AuthorityIdentifier,
        round: RandomnessRound,
        sigs: Vec<RandomnessPartialSignature>,
    ) {
        let vss_pk = {
            let Some(dkg_output) = &self.dkg_output else {
                error!("random beacon: called receive_partial_signatures before DKG completed");
                return;
            };
            &dkg_output.vss_pk
        };
        let randomness_round = self.store.randomness_round();
        if round < randomness_round {
            debug!(
                "random beacon: ignoring partial signatures for old round {round} (we are at {randomness_round})",
            );
            return;
        }
        // We may get newer partial signatures if this node is behind relative to others.
        // Instead of throwing them away, we store up to a couple rounds ahead so they can
        // be used if we catch up.
        const MAX_ROUND_DELTA: u64 = 2;
        if round > randomness_round + MAX_ROUND_DELTA {
            debug!(
                "random beacon: ignoring partial signatures for too-new round {round} (we are at {randomness_round})",
            );
            return;
        }
        if let Some(last_sent) = self.last_randomness_round_sent {
            if last_sent >= randomness_round {
                debug!(
                    "random beacon: ignoring partial signatures for already-finished round {round}"
                );
                return;
            }
        }
        if self
            .partial_sigs
            .insert((round, authority_id), sigs)
            .is_some()
        {
            debug!("random beacon: replacing existing partial signatures from authority {authority_id} for round {round}");
        }

        // If we have enough partial signatures, aggregate them.
        let mut sig = match ThresholdBls12381MinSig::aggregate(
            self.party.t(),
            self.partial_sigs
                .iter()
                .filter(|&((round, _), _)| *round == randomness_round)
                .flat_map(|(_, sigs)| sigs),
        ) {
            Ok(sig) => sig,
            Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => return, // wait for more input
            Err(e) => {
                error!("Error while aggregating randomness partial signatures: {e:?}");
                return;
            }
        };

        // Try to verify the aggregated signature all at once. (Should work in the happy path.)
        if ThresholdBls12381MinSig::verify(vss_pk.c0(), &round.signature_message(), &sig).is_err() {
            // If verifiation fails, some of the inputs must be invalid. We have to go through
            // one-by-one to find which.
            // TODO: add test for individual sig verification.
            self.partial_sigs.retain(|&(r, authority_id), partial_sigs| {
                if ThresholdBls12381MinSig::partial_verify_batch(
                    vss_pk,
                    &r.signature_message(),
                     partial_sigs.iter(),
                    &mut rand::thread_rng(),
                )
                .is_err()
                {
                    warn!("Received invalid partial signatures from possibly-Byzantine authority {authority_id}");
                    return false
                }
                true
            });
            sig = match ThresholdBls12381MinSig::aggregate(
                self.party.t(),
                self.partial_sigs
                    .iter()
                    .filter(|&((round, _), _)| *round == randomness_round)
                    .flat_map(|(_, sigs)| sigs),
            ) {
                Ok(sig) => sig,
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => return, // wait for more input
                Err(e) => {
                    error!("Error while aggregating randomness partial signatures: {e:?}");
                    return;
                }
            };
            if let Err(e) =
                ThresholdBls12381MinSig::verify(vss_pk.c0(), &round.signature_message(), &sig)
            {
                error!("Error while verifying randomness partial signatures after removing invalid partials: {e:?}");
                return;
            }
        }

        let _ = self
            .tx_system_messages
            .send(SystemMessage::RandomnessSignature(
                randomness_round,
                bcs::to_bytes(&sig).expect("signature serialization should not fail"),
            ))
            .await;
        self.last_randomness_round_sent = Some(randomness_round);
    }
}

impl StateHandler {
    #[must_use]
    pub fn spawn(
        chain: &ChainIdentifier,
        protocol_config: &ProtocolConfig,
        authority_id: AuthorityIdentifier,
        committee: Committee,
        rx_committed_certificates: Receiver<(Round, Vec<Certificate>)>,
        rx_randomness_partial_signatures: Receiver<(
            AuthorityIdentifier,
            RandomnessRound,
            Vec<RandomnessPartialSignature>,
        )>,
        rx_shutdown: ConditionalBroadcastReceiver,
        rx_narwhal_round_updates: watch::Receiver<Round>,
        tx_committed_own_headers: Option<Sender<(Round, Vec<Round>)>>,
        vss_key_output: Arc<OnceLock<PublicVssKey>>,
        tx_system_messages: Sender<SystemMessage>,
        randomness_private_key: RandomnessPrivateKey,
        leader_schedule: LeaderSchedule,
        network: anemo::Network,
        randomness_store: RandomnessStore,
        metrics: Arc<PrimaryMetrics>,
    ) -> JoinHandle<()> {
        let state_handler = Self {
            authority_id,
            rx_committed_certificates,
            rx_shutdown,
            rx_narwhal_round_updates: WatchStream::from(rx_narwhal_round_updates),
            rx_randomness_partial_signatures,
            tx_committed_own_headers,
            randomness_state: RandomnessState::try_new(
                chain,
                protocol_config,
                committee,
                authority_id,
                randomness_private_key,
                leader_schedule,
                network.clone(),
                vss_key_output,
                tx_system_messages,
                randomness_store,
                metrics,
            ),
            network,
        };
        spawn_logged_monitored_task!(
            async move {
                state_handler.run().await;
            },
            "StateHandlerTask"
        )
    }

    async fn handle_sequenced(&mut self, commit_round: Round, certificates: Vec<Certificate>) {
        debug!(
            "state handler: received {:?} sequenced certificates at round {commit_round}",
            certificates.len()
        );

        // Now we are going to signal which of our own batches have been committed.
        let own_rounds_committed: Vec<_> = certificates
            .iter()
            .filter_map(|cert| {
                if cert.header().author() == self.authority_id {
                    Some(cert.header().round())
                } else {
                    None
                }
            })
            .collect();
        debug!(
            "Own committed rounds {:?} at round {:?}",
            own_rounds_committed, commit_round
        );

        // If a reporting channel is available send the committed own
        // headers to it.
        if let Some(sender) = &self.tx_committed_own_headers {
            let _ = sender.send((commit_round, own_rounds_committed)).await;
        }

        // Process committed system messages.
        if let Some(randomness_state) = self.randomness_state.as_mut() {
            for certificate in certificates {
                let header = certificate.header();
                for message in header.system_messages() {
                    type DkgG = <ThresholdBls12381MinSig as ThresholdBls>::Public;
                    match message {
                        SystemMessage::DkgMessage(bytes) => {
                            let msg: fastcrypto_tbls::dkg::Message<DkgG, DkgG> = bcs::from_bytes(
                                bytes,
                            )
                            .expect(
                                "DKG message deserialization from certified header should not fail",
                            );
                            randomness_state.add_message(msg.clone());
                        }
                        SystemMessage::DkgConfirmation(bytes) => {
                            let conf: fastcrypto_tbls::dkg::Confirmation<DkgG> =
                                bcs::from_bytes(bytes).expect(
                                    "DKG confirmation deserialization from certified header should not fail",
                                );
                            randomness_state.add_confirmation(conf.clone())
                        }
                        SystemMessage::RandomnessSignature(round, _bytes) => {
                            randomness_state.update_randomness_round(*round + 1).await;
                        }
                    }
                }
                // Advance the random beacon DKG protocol if possible after each certificate.
                randomness_state.advance_dkg().await;
            }
        }
    }

    async fn run(mut self) {
        info!(
            "StateHandler on node {} has started successfully.",
            self.authority_id
        );

        // Kick off randomness DKG if enabled.
        if let Some(randomness_state) = self.randomness_state.as_mut() {
            randomness_state.start_dkg().await;
            randomness_state.advance_dkg().await; // for crash recovery
        }

        loop {
            tokio::select! {
                biased;

                _ = self.rx_shutdown.receiver.recv() => {
                    // shutdown network
                    let _ = self.network.shutdown().await.tap_err(|err|{
                        error!("Error while shutting down network: {err}")
                    });

                    warn!("Network has shutdown");

                    return;
                }

                Some(round) = self.rx_narwhal_round_updates.next() => {
                    if let Some(randomness_state) = self.randomness_state.as_mut() {
                        randomness_state.update_narwhal_round(round).await;
                    }
                }

                Some((commit_round, certificates)) = self.rx_committed_certificates.recv() => {
                    self.handle_sequenced(commit_round, certificates).await;
                },

                Some(
                    (authority_id, round, sigs)
                ) = self.rx_randomness_partial_signatures.recv() => {
                    if let Some(randomness_state) = self.randomness_state.as_mut() {
                        randomness_state.receive_partial_signatures(
                            authority_id,
                            round,
                            sigs
                        ).await;
                    }
                }
            }
        }
    }
}
