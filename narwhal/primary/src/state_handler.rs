// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{AuthorityIdentifier, ChainIdentifier, Committee};
use consensus::consensus::LeaderSchedule;
use crypto::{RandomnessPartialSignature, RandomnessPrivateKey};
use fastcrypto::groups;
use fastcrypto_tbls::tbls::ThresholdBls;
use fastcrypto_tbls::types::{PublicVssKey, ThresholdBls12381MinSig};
use fastcrypto_tbls::{dkg, nodes};
use mysten_metrics::metered_channel::{Receiver, Sender};
use mysten_metrics::spawn_logged_monitored_task;
use network::anemo_ext::NetworkExt;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use sui_protocol_config::ProtocolConfig;
use tap::TapFallible;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::WatchStream;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};
use types::{
    Certificate, CertificateAPI, ConditionalBroadcastReceiver, HeaderAPI, PrimaryToPrimaryClient,
    Round, SendRandomnessPartialSignaturesRequest, SystemMessage,
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
    /// Channel to recieve partial signatures for randomness generation.
    rx_randomness_partial_signatures:
        Receiver<(AuthorityIdentifier, u64, Vec<RandomnessPartialSignature>)>,
    /// A channel to update the committed rounds
    tx_committed_own_headers: Option<Sender<(Round, Vec<Round>)>>,

    /// If set, generates Narwhal system messages for random beacon
    /// DKG and randomness generation.
    randomness_state: Option<RandomnessState>,

    network: anemo::Network,
}

// Internal state for randomness DKG and generation.
// TODO: Write a brief protocol description.
struct RandomnessState {
    // A channel to send system messages to the proposer.
    tx_system_messages: Sender<SystemMessage>,

    // State for DKG.
    party: dkg::Party<PkG, EncG>,
    processed_messages: Vec<dkg::ProcessedMessage<PkG, EncG>>,
    used_messages: Option<dkg::UsedProcessedMessages<PkG, EncG>>,
    confirmations: Vec<dkg::Confirmation<EncG>>,
    dkg_output: Option<dkg::Output<PkG, EncG>>,
    vss_key_output: Arc<OnceLock<PublicVssKey>>,

    // State for randomness generation.
    leader_schedule: LeaderSchedule,
    network: anemo::Network,
    last_randomness_round_sent: Option<u64>,
    randomness_round: u64,
    last_narwhal_round_sent: Round,
    narhwal_round: Round,
    partial_sigs: BTreeMap<AuthorityIdentifier, Vec<RandomnessPartialSignature>>,
}

impl RandomnessState {
    // Returns None in case of invalid input or other failure to initialize DKG.
    // In this case, narwhal will continue to function normally and simpluy not run
    // the random beacon protocol during the current epoch.
    fn try_new(
        chain: &ChainIdentifier,
        protocol_config: &ProtocolConfig,
        committee: Committee,
        private_key: RandomnessPrivateKey,
        leader_schedule: LeaderSchedule,
        network: anemo::Network,
        // Writes the VSS public key to this lock once DKG completes.
        vss_key_output: Arc<OnceLock<PublicVssKey>>,
        tx_system_messages: Sender<SystemMessage>,
    ) -> Option<Self> {
        if !protocol_config.random_beacon() {
            info!("random beacon: disabled");
            return None;
        }

        let info = committee.randomness_dkg_info();
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
                error!("Error while initializing random beacon Nodes: {err:?}");
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
        let total_weight = nodes.n();
        let party = match dkg::Party::<PkG, EncG>::new(
            private_key,
            nodes,
            t.into(),
            fastcrypto_tbls::random_oracle::RandomOracle::new(
                format!("dkg {:x?} {}", chain.as_bytes(), committee.epoch()).as_str(),
            ),
            &mut rand::thread_rng(),
        ) {
            Ok(party) => party,
            Err(err) => {
                error!("Error while initializing random beacon Party: {err:?}");
                return None;
            }
        };
        info!("random beacon: state initialized with total_weight={total_weight}, t={t}");
        Some(Self {
            tx_system_messages,
            party,
            processed_messages: Vec::new(),
            used_messages: None,
            confirmations: Vec::new(),
            dkg_output: None,
            vss_key_output,
            leader_schedule,
            network,
            last_randomness_round_sent: None,
            randomness_round: 0,
            last_narwhal_round_sent: 0,
            narhwal_round: 0,
            partial_sigs: BTreeMap::new(),
        })
    }

    async fn start_dkg(&self) {
        let msg = self.party.create_message(&mut rand::thread_rng());
        info!("random beacon: sending DKG Message: {msg:?}");
        let _ = self
            .tx_system_messages
            .send(SystemMessage::DkgMessage(msg))
            .await;
    }

    fn add_message(&mut self, msg: dkg::Message<PkG, EncG>) {
        if self.used_messages.is_some() {
            // We've already sent a `Confirmation`, so we can't add any more messages.
            return;
        }
        match self.party.process_message(msg, &mut rand::thread_rng()) {
            Ok(processed) => {
                self.processed_messages.push(processed);
            }
            Err(err) => {
                debug!("error while processing randomness DKG message: {err:?}");
            }
        }
    }

    fn add_confirmation(&mut self, conf: dkg::Confirmation<EncG>) {
        if self.used_messages.is_none() {
            // We should never see a `Confirmation` before we've sent our `Message` because
            // DKG messages are processed in consensus order.
            return;
        }
        if self.dkg_output.is_some() {
            // Once we have completed DKG, no more `Confirmation`s are needed.
            return;
        }
        self.confirmations.push(conf)
    }

    // Generates the next SystemMessage needed to advance the random beacon DKG protocol, if
    // possible, and sends it to the proposer.
    async fn advance_dkg(&mut self) {
        // Once we have enough ProcessedMessages, send a Confirmation.
        if self.used_messages.is_none() && !self.processed_messages.is_empty() {
            match self.party.merge(&self.processed_messages) {
                Ok((conf, used_msgs)) => {
                    info!(
                        "random beacon: sending DKG Confirmation with {} complaints",
                        conf.complaints.len()
                    );
                    self.used_messages = Some(used_msgs);
                    let _ = self
                        .tx_system_messages
                        .send(SystemMessage::DkgConfirmation(conf))
                        .await;
                }
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => (), // wait for more input
                Err(e) => debug!("Error while merging randomness DKG messages: {e:?}"),
            }
        }

        // Once we have enough Confirmations, process them and update shares.
        if self.dkg_output.is_none()
            && !self.confirmations.is_empty()
            && self.used_messages.is_some()
        {
            match self.party.complete(
                self.used_messages.as_ref().expect("checked above"),
                &self.confirmations,
                self.party.t() * 2 - 1, // t==f+1, we want 2f+1
                &mut rand::thread_rng(),
            ) {
                Ok(output) => {
                    if let Err(e) = self.vss_key_output.set(output.vss_pk.clone()) {
                        error!("random beacon: unable to write VSS key to output: {e:?}")
                    }
                    self.dkg_output = Some(output);
                    info!(
                        "random beacon: DKG complete with Output {:?}",
                        self.dkg_output
                    );
                    // Begin randomenss generation.
                    self.send_partial_signatures();
                }
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => (), // wait for more input
                Err(e) => error!("Error while processing randomness DKG confirmations: {e:?}"),
            }
        }
    }

    fn update_narwhal_round(&mut self, round: Round) {
        self.narhwal_round = round;
        let next_leader_narwhal_round = round + (round % 2);
        // Re-send partial signatures to new leader, in case the last one failed.
        if self.dkg_output.is_some() && (next_leader_narwhal_round > self.last_narwhal_round_sent) {
            self.send_partial_signatures();
        }
    }

    fn update_randomness_round(&mut self, round: u64) {
        self.randomness_round = round;
        self.partial_sigs.clear();
        self.send_partial_signatures();
    }

    fn send_partial_signatures(&mut self) {
        let dkg_output = match &self.dkg_output {
            Some(dkg_output) => dkg_output,
            None => {
                error!("random beacon: called send_partial_signatures before DKG completed");
                return;
            }
        };
        let shares = match &dkg_output.shares {
            Some(shares) => shares,
            None => return, // can't participate in randomness generation without shares
        };
        debug!(
            "random beacon: sending partial signatures for round {}",
            self.randomness_round
        );
        let sigs = ThresholdBls12381MinSig::partial_sign_batch(
            shares,
            self.randomness_round.to_be_bytes().as_slice(),
        );
        let next_leader_narwhal_round = self.narhwal_round + (self.narhwal_round % 2);
        self.last_narwhal_round_sent = next_leader_narwhal_round;

        let leader = self.leader_schedule.leader(next_leader_narwhal_round);
        let peer_id = anemo::PeerId(leader.network_key().0.to_bytes());
        let peer = self.network.waiting_peer(peer_id);
        let mut client = PrimaryToPrimaryClient::new(peer);
        const SEND_PARTIAL_SIGNATURES_TIMEOUT: Duration = Duration::from_secs(10);
        let request = anemo::Request::new(SendRandomnessPartialSignaturesRequest {
            round: self.randomness_round,
            sigs,
        })
        .with_timeout(SEND_PARTIAL_SIGNATURES_TIMEOUT);

        // TODO: Consider canceling previous oustanding task to send sigs when a new one is started.
        spawn_logged_monitored_task!(
            async move {
                let resp = client.send_randomness_partial_signatures(request).await;
                if let Err(e) = resp {
                    info!(
                        "random beacon: error sending partial signatures to leader {leader:?}: {e:?}"
                    );
                }
            },
            "RandomnessSendPartialSignatures"
        );
    }

    async fn receive_partial_signatures(
        &mut self,
        authority_id: AuthorityIdentifier,
        round: u64,
        sigs: Vec<RandomnessPartialSignature>,
    ) {
        let dkg_output = match &self.dkg_output {
            Some(dkg_output) => dkg_output,
            None => {
                error!("random beacon: called receive_partial_signatures before DKG completed");
                return;
            }
        };
        if round != self.randomness_round {
            debug!("random beacon: ignoring partial signatures for non-matching round {round}");
            return;
        }
        if let Some(last_sent) = self.last_randomness_round_sent {
            if last_sent >= self.randomness_round {
                debug!(
                    "random beacon: ignoring partial signatures for already-finished round {round}"
                );
                return;
            }
        }
        if let Err(e) = ThresholdBls12381MinSig::partial_verify_batch(
            &dkg_output.vss_pk,
            round.to_be_bytes().as_slice(),
            sigs.as_slice(),
            &mut rand::thread_rng(),
        ) {
            debug!("random beacon: ignoring partial signatures with verification error: {e:?}");
        }
        if self.partial_sigs.insert(authority_id, sigs).is_some() {
            debug!("random beacon: replacing existing partial signatures from authority {authority_id} for round {round}");
        }

        // If we have enough partial signatures, aggregate them and send to consensus.
        let sig = match ThresholdBls12381MinSig::aggregate(
            self.party.t(),
            // TODO: ThresholdBls12381MinSig::aggregate immediately just makes an iterator of the
            // given slice. Can we change its interface to accept an iterator directly, to avoid
            // all the extra copying?
            &self
                .partial_sigs
                .values()
                .flatten()
                .cloned()
                .collect::<Vec<_>>(),
        ) {
            Ok(sig) => sig,
            Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => return, // wait for more input
            Err(e) => {
                error!("Error while aggregating randomness partial signatures: {e:?}");
                return;
            }
        };
        let _ = self
            .tx_system_messages
            .send(SystemMessage::RandomnessSignature(
                self.randomness_round,
                sig,
            ))
            .await;
        self.last_randomness_round_sent = Some(self.randomness_round);
        self.partial_sigs.clear();
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
            u64,
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
                randomness_private_key,
                leader_schedule,
                network.clone(),
                vss_key_output,
                tx_system_messages,
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
                    match message {
                        SystemMessage::DkgMessage(msg) => randomness_state.add_message(msg.clone()),
                        SystemMessage::DkgConfirmation(conf) => {
                            randomness_state.add_confirmation(conf.clone())
                        }
                        SystemMessage::RandomnessSignature(round, _sig) => {
                            randomness_state.update_randomness_round(round + 1);
                        }
                    }
                }
                // Advance the random beacon protocol if possible after each certificate.
                // TODO: Implement/audit crash recovery for random beacon.
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
        if let Some(ref randomness_state) = self.randomness_state {
            randomness_state.start_dkg().await;
        }

        loop {
            tokio::select! {
                Some((commit_round, certificates)) = self.rx_committed_certificates.recv() => {
                    self.handle_sequenced(commit_round, certificates).await;
                },

                Some(round) = self.rx_narwhal_round_updates.next() => {
                    if let Some(randomness_state) = self.randomness_state.as_mut() {
                        randomness_state.update_narwhal_round(round);
                    }
                }

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

                _ = self.rx_shutdown.receiver.recv() => {
                    // shutdown network
                    let _ = self.network.shutdown().await.tap_err(|err|{
                        error!("Error while shutting down network: {err}")
                    });

                    warn!("Network has shutdown");

                    return;
                }
            }
        }
    }
}
