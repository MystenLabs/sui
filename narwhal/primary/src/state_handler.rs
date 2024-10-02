// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::AuthorityIdentifier;
use mysten_metrics::metered_channel::{Receiver, Sender};
use mysten_metrics::spawn_logged_monitored_task;
use tap::TapFallible;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use types::{Certificate, CertificateAPI, ConditionalBroadcastReceiver, HeaderAPI, Round};

/// Updates Narwhal system state based on certificates received from consensus.
pub struct StateHandler {
    authority_id: AuthorityIdentifier,

    /// Receives the ordered certificates from consensus.
    rx_committed_certificates: Receiver<(Round, Vec<Certificate>)>,
    /// Channel to signal committee changes.
    rx_shutdown: ConditionalBroadcastReceiver,
    /// A channel to update the committed rounds
    tx_committed_own_headers: Option<Sender<(Round, Vec<Round>)>>,

    network: anemo::Network,
}

impl StateHandler {
    #[must_use]
    pub fn spawn(
        authority_id: AuthorityIdentifier,
        rx_committed_certificates: Receiver<(Round, Vec<Certificate>)>,
        rx_shutdown: ConditionalBroadcastReceiver,
        tx_committed_own_headers: Option<Sender<(Round, Vec<Round>)>>,
        network: anemo::Network,
    ) -> JoinHandle<()> {
        let state_handler = Self {
            authority_id,
            rx_committed_certificates,
            rx_shutdown,
            tx_committed_own_headers,
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
    }

    async fn run(mut self) {
        info!(
            "StateHandler on node {} has started successfully.",
            self.authority_id
        );

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

                Some((commit_round, certificates)) = self.rx_committed_certificates.recv() => {
                    self.handle_sequenced(commit_round, certificates).await;
                },
            }
        }
    }
}
