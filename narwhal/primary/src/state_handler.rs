// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::SharedWorkerCache;
use crypto::PublicKey;
use mysten_metrics::spawn_logged_monitored_task;
use network::{CancelOnDropHandler, ReliableNetwork};
use tap::TapFallible;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use types::{
    metered_channel::{Receiver, Sender},
    Certificate, PreSubscribedBroadcastSender, ReconfigureNotification, Round,
    WorkerReconfigureMessage,
};

/// Receives the highest round reached by consensus and update it for all tasks.
pub struct StateHandler {
    /// The public key of this authority.
    name: PublicKey,
    /// The worker information cache.
    worker_cache: SharedWorkerCache,
    /// Receives the ordered certificates from consensus.
    rx_committed_certificates: Receiver<(Round, Vec<Certificate>)>,
    /// Receives notifications to reconfigure the system.
    rx_state_handler: Receiver<ReconfigureNotification>,
    /// Channel to signal committee changes.
    tx_shutdown: PreSubscribedBroadcastSender,
    /// A channel to update the committed rounds
    tx_commited_own_headers: Option<Sender<(Round, Vec<Round>)>>,

    network: anemo::Network,
}

impl StateHandler {
    #[must_use]
    pub fn spawn(
        name: PublicKey,
        worker_cache: SharedWorkerCache,
        rx_committed_certificates: Receiver<(Round, Vec<Certificate>)>,
        rx_state_handler: Receiver<ReconfigureNotification>,
        tx_shutdown: PreSubscribedBroadcastSender,
        tx_commited_own_headers: Option<Sender<(Round, Vec<Round>)>>,
        network: anemo::Network,
    ) -> JoinHandle<()> {
        spawn_logged_monitored_task!(
            async move {
                Self {
                    name,
                    worker_cache,
                    rx_committed_certificates,
                    rx_state_handler,
                    tx_shutdown,
                    tx_commited_own_headers,
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
                if cert.header.author == self.name {
                    Some(cert.header.round)
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
        if let Some(sender) = &self.tx_commited_own_headers {
            let _ = sender.send((commit_round, own_rounds_committed)).await;
        }
    }

    fn notify_our_workers(
        &mut self,
        message: ReconfigureNotification,
    ) -> Vec<CancelOnDropHandler<anyhow::Result<anemo::Response<()>>>> {
        let message = WorkerReconfigureMessage { message };
        let our_workers = self
            .worker_cache
            .load()
            .our_workers(&self.name)
            .unwrap()
            .into_iter()
            .map(|info| info.name)
            .collect();

        self.network.broadcast(our_workers, &message)
    }

    async fn run(mut self) {
        info!(
            "StateHandler on node {} has started successfully.",
            self.name
        );
        loop {
            tokio::select! {
                Some((commit_round, certificates)) = self.rx_committed_certificates.recv() => {
                    self.handle_sequenced(commit_round, certificates).await;
                },

                Some(message) = self.rx_state_handler.recv() => {
                    // Notify our workers
                    let notify_handlers = self.notify_our_workers(message.to_owned());



                    // Notify all other tasks.
                    self.tx_shutdown
                        .send()
                        .expect("Shutdown channel dropped");

                    warn!("Waiting to broadcast shutdown message to workers");

                    // wait for all the workers to eventually receive the message
                    // TODO: this request will be removed https://mysten.atlassian.net/browse/SUI-984
                    let join_all = futures::future::try_join_all(notify_handlers);
                    join_all.await.expect("Error while sending reconfiguration message to the workers");

                    warn!("Successfully broadcasted reconfigure message to workers");

                    // Exit only when we are sure that all the other tasks received
                    // the shutdown message.

                    // shutdown network as well
                    let _ = self.network.shutdown().await.tap_err(|err|{
                        error!("Error while shutting down network: {err}")
                    });

                    warn!("Network has shutdown");

                    // self.tx_shutdown.closed().await;

                    //warn!("All reconfiguration receivers dropped");

                    return;

                }
            }
        }
    }
}
