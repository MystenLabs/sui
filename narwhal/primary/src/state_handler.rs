// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{AuthorityIdentifier, Committee};
use crypto::RandomnessPrivateKey;
use fastcrypto::groups;
use fastcrypto_tbls::dkg;
use mysten_metrics::metered_channel::{Receiver, Sender};
use mysten_metrics::spawn_logged_monitored_task;
use tap::TapFallible;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use types::{
    Certificate, CertificateAPI, ConditionalBroadcastReceiver, HeaderAPI, Round, SystemMessage,
};

type G = groups::bls12381::G2Element; // TODO-DNS is it correct for these to be the same?
type EG = groups::bls12381::G2Element;

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

    /// If set, generates Narwhal system messages for random beacn
    /// DKG and randomness generation.
    randomness_state: Option<RandomnessState>,

    network: anemo::Network,
}

// Internal state for randomness DKG and generation.
struct RandomnessState {
    party: dkg::Party<G, EG>,
    messages: Vec<dkg::Message<G, EG>>,
    processed_messages: Vec<dkg::ProcessedMessage<G, EG>>,
    shares: dkg::SharesMap<<G as groups::GroupElement>::ScalarType>,
    confirmations: Vec<dkg::Confirmation<EG>>,
    dkg_output: Option<dkg::Output<G, EG>>,
}

impl RandomnessState {
    fn try_new(committee: Committee, private_key: RandomnessPrivateKey) -> Option<Self> {
        let total_stake = committee.total_stake() as f64;
        let info = committee.randomness_dkg_info();

        let nodes = info
            .iter()
            .map(|(id, pk, stake)| fastcrypto_tbls::nodes::Node::<EG> {
                id: id.0,
                pk: pk.clone(),
                weight: ((*stake as f64 / total_stake) * 2000.0) as u16, // TODO-DNS update to reduce in fastcrypto
            })
            .collect();
        let party = match dkg::Party::<G, EG>::new(
            private_key,
            nodes,
            (info.len() / 3 + 1) as u32, // TODO-DNS - what to use for `t`?
            fastcrypto_tbls::random_oracle::RandomOracle::new("dkg"),
            &mut rand::thread_rng(),
        ) {
            Ok(party) => party,
            Err(err) => {
                error!("Error while initializing random beacon state: {err:?}");
                return None;
            }
        };
        Some(Self {
            party,
            messages: Vec::new(),
            processed_messages: Vec::new(),
            shares: dkg::SharesMap::with_capacity(0),
            confirmations: Vec::new(),
            dkg_output: None,
        })
    }

    async fn start_dkg(&self, tx_system_messages: &Sender<SystemMessage>) {
        let msg = self.party.create_message(&mut rand::thread_rng());
        let _ = tx_system_messages
            .send(SystemMessage::DkgMessage(msg))
            .await;
    }

    fn add_message(&mut self, msg: dkg::Message<G, EG>) {
        if !self.shares.is_empty() {
            // We've already sent a `Confirmation`, so we can't add any more messages.
            return;
        }
        self.messages.push(msg.clone());
        match self.party.process_message(msg, &mut rand::thread_rng()) {
            Ok(processed) => {
                self.processed_messages.push(processed);
            }
            Err(err) => {
                debug!("error while processing randomness DKG message: {err:?}");
            }
        }
    }

    fn add_confirmation(&mut self, conf: dkg::Confirmation<EG>) {
        self.confirmations.push(conf)
    }

    // Generates the next SystemMessage needed to advance the random beacon protocol, if possible,
    // and sends it to the proposer.
    async fn advance(&mut self, tx_system_messages: &Sender<SystemMessage>) {
        // Once we have enough ProcessedMessages, send a Confirmation.
        if self.shares.is_empty() && !self.processed_messages.is_empty() {
            // TODO-DNS make sure fastcrypto code is updated to dedupe processed messages
            // instead of returning error
            match self.party.merge(&self.processed_messages) {
                Ok((shares, conf)) => {
                    self.shares = shares;
                    let _ = tx_system_messages
                        .send(SystemMessage::DkgConfirmation(conf))
                        .await;
                }
                Err(fastcrypto::error::FastCryptoError::InputTooShort(_)) => (), // wait for more input
                Err(e) => error!("Error while merging randomness DKG messages: {e:?}"),
            }
        }

        // Once we have enough Confirmations, process them and update shares.
        if self.dkg_output.is_none() && !self.confirmations.is_empty() {
            match self.party.process_confirmations(
                &self.messages,
                &self.confirmations,
                self.shares.clone(),
                todo!(), // TODO-DNS - how to set threshold?
                &mut rand::thread_rng(),
            ) {
                Ok(shares) => {
                    self.dkg_output = Some(self.party.aggregate(&self.messages, shares));
                }
                Err(fastcrypto::error::FastCryptoError::InputTooShort(_)) => (), // wait for more input
                Err(e) => error!("Error while processing randomness DKG confirmations: {e:?}"),
            }
        }
    }
}

impl StateHandler {
    #[must_use]
    pub fn spawn(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        rx_committed_certificates: Receiver<(Round, Vec<Certificate>)>,
        rx_shutdown: ConditionalBroadcastReceiver,
        tx_committed_own_headers: Option<Sender<(Round, Vec<Round>)>>,
        tx_system_messages: Sender<SystemMessage>,
        randomness_private_key: Option<RandomnessPrivateKey>,
        network: anemo::Network,
    ) -> JoinHandle<()> {
        spawn_logged_monitored_task!(
            async move {
                Self {
                    authority_id,
                    rx_committed_certificates,
                    rx_shutdown,
                    tx_committed_own_headers,
                    tx_system_messages,
                    randomness_state: randomness_private_key
                        .and_then(|key| RandomnessState::try_new(committee, key)),
                    network,
                }
                .run()
                .await;
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
            }
            // Once all messages in the new commit are saved, advance the random
            // beacon protocol if possible.
            randomness_state.advance(&self.tx_system_messages).await;
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
