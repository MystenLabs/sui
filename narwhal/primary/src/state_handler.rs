// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{AuthorityIdentifier, ChainIdentifier, Committee};
use crypto::RandomnessPrivateKey;
use fastcrypto::groups;
use fastcrypto_tbls::{dkg, nodes};
use mysten_metrics::metered_channel::{Receiver, Sender};
use mysten_metrics::spawn_logged_monitored_task;
use sui_protocol_config::ProtocolConfig;
use tap::TapFallible;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use types::{
    Certificate, CertificateAPI, ConditionalBroadcastReceiver, HeaderAPI, Round, SystemMessage,
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
    /// A channel to update the committed rounds
    tx_committed_own_headers: Option<Sender<(Round, Vec<Round>)>>,
    /// A channel to send system messages to the proposer.
    tx_system_messages: Sender<SystemMessage>,

    /// If set, generates Narwhal system messages for random beacon
    /// DKG and randomness generation.
    randomness_state: Option<RandomnessState>,

    network: anemo::Network,
}

// Internal state for randomness DKG and generation.
// TODO: Write a brief protocol description.
struct RandomnessState {
    party: dkg::Party<PkG, EncG>,
    processed_messages: Vec<dkg::ProcessedMessage<PkG, EncG>>,
    used_messages: Option<dkg::UsedProcessedMessages<PkG, EncG>>,
    confirmations: Vec<dkg::Confirmation<EncG>>,
    dkg_output: Option<dkg::Output<PkG, EncG>>,
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
            party,
            processed_messages: Vec::new(),
            used_messages: None,
            confirmations: Vec::new(),
            dkg_output: None,
        })
    }

    async fn start_dkg(&self, tx_system_messages: &Sender<SystemMessage>) {
        let msg = self.party.create_message(&mut rand::thread_rng());
        info!("random beacon: sending DKG Message: {msg:?}");
        let _ = tx_system_messages
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

    // Generates the next SystemMessage needed to advance the random beacon protocol, if possible,
    // and sends it to the proposer.
    async fn advance(&mut self, tx_system_messages: &Sender<SystemMessage>) {
        // Once we have enough ProcessedMessages, send a Confirmation.
        if self.used_messages.is_none() && !self.processed_messages.is_empty() {
            match self.party.merge(&self.processed_messages) {
                Ok((conf, used_msgs)) => {
                    info!(
                        "random beacon: sending DKG Confirmation with {} complaints",
                        conf.complaints.len()
                    );
                    self.used_messages = Some(used_msgs);
                    let _ = tx_system_messages
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
                    self.dkg_output = Some(output);
                    info!(
                        "random beacon: DKG complete with Output {:?}",
                        self.dkg_output
                    );
                }
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => (), // wait for more input
                Err(e) => error!("Error while processing randomness DKG confirmations: {e:?}"),
            }
        }
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
        rx_shutdown: ConditionalBroadcastReceiver,
        tx_committed_own_headers: Option<Sender<(Round, Vec<Round>)>>,
        tx_system_messages: Sender<SystemMessage>,
        randomness_private_key: RandomnessPrivateKey,
        network: anemo::Network,
    ) -> JoinHandle<()> {
        let state_handler = Self {
            authority_id,
            rx_committed_certificates,
            rx_shutdown,
            tx_committed_own_headers,
            tx_system_messages,
            randomness_state: RandomnessState::try_new(
                chain,
                protocol_config,
                committee,
                randomness_private_key,
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
                    }
                }
                // Advance the random beacon protocol if possible after each certificate.
                // TODO: Implement/audit crash recovery for random beacon.
                randomness_state.advance(&self.tx_system_messages).await;
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
            randomness_state.start_dkg(&self.tx_system_messages).await;
        }

        loop {
            tokio::select! {
                Some((commit_round, certificates)) = self.rx_committed_certificates.recv() => {
                    self.handle_sequenced(commit_round, certificates).await;
                },

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
