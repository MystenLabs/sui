use std::{sync::Arc, time::Duration};

use anemo::{rpc::Status, Request, Response};
use config::{AuthorityIdentifier, Committee};
use crypto::NetworkPublicKey;
use futures::{stream::FuturesUnordered, StreamExt};
use mysten_metrics::spawn_logged_monitored_task;
use network::{
    anemo_ext::{NetworkExt, WaitingPeer},
    client::NetworkClient,
};
use parking_lot::Mutex;
use tokio::{sync::broadcast, task::JoinSet, time::sleep};
use tracing::{trace, warn};
use types::{PrimaryToPrimaryClient, SendHeaderRequest, SendHeaderResponse, SignedHeader};

const BROADCAST_BACKLOG_CAPACITY: usize = 10000;
const BROADCAST_CONCURRENCY: usize = 100;

/// Broadcaster ensures headers are broadcasted to other primaries with retries for network errors.
/// Also, Broadcaster will keep broadcasting the latest header to help the network stay alive.
pub struct Broadcaster {
    inner: Arc<Inner>,
}

impl Broadcaster {
    pub(crate) fn new(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        client: NetworkClient,
    ) -> Self {
        let (tx_own_header_broadcast, _rx_own_header_broadcast) =
            broadcast::channel(BROADCAST_BACKLOG_CAPACITY);
        let inner = Arc::new(Inner {
            authority_id,
            committee,
            client,
            header_senders: Default::default(),
            tx_own_header_broadcast: tx_own_header_broadcast.clone(),
        });

        // Initialize sender tasks asynchronously, to not block construction of Broadcaster.
        let inner_clone = inner.clone();
        spawn_logged_monitored_task!(
            async move {
                let mut senders = inner_clone.header_senders.lock();
                for (peer_authority, _, peer_name) in inner_clone
                    .committee
                    .others_primaries_by_id(inner_clone.authority_id)
                    .into_iter()
                {
                    senders.spawn(Self::push_headers(
                        inner_clone.clone(),
                        peer_authority,
                        peer_name,
                        tx_own_header_broadcast.subscribe(),
                    ));
                }
            },
            "Broadcaster"
        );
        Self { inner }
    }

    pub(crate) fn broadcast_header(&self, signed_header: SignedHeader) {
        if let Err(e) = self.inner.tx_own_header_broadcast.send(signed_header) {
            warn!(
                "Failed to broadcast header. Likely all senders have exited. ({:?})",
                e
            );
        }
    }

    /// Runs a loop that continously pushes new headers received from the rx_own_header_broadcast
    /// channel to the target peer.
    ///
    /// Exits only when the primary is shutting down.
    async fn push_headers(
        inner: Arc<Inner>,
        peer_authority: AuthorityIdentifier,
        peer_name: NetworkPublicKey,
        mut rx_own_header_broadcast: broadcast::Receiver<SignedHeader>,
    ) {
        let network = inner.client.get_primary_network().await.unwrap();
        const PUSH_TIMEOUT: Duration = Duration::from_secs(10);
        let peer_id = anemo::PeerId(peer_name.0.to_bytes());
        let peer = network.waiting_peer(peer_id);
        let client = PrimaryToPrimaryClient::new(peer);
        // This will contain at most headers created within the last PUSH_TIMEOUT.
        let mut requests = FuturesUnordered::new();
        const BACKOFF_INTERVAL: Duration = Duration::from_millis(100);
        const MAX_BACKOFF_MULTIPLIER: u32 = 50;

        async fn send_header(
            mut client: PrimaryToPrimaryClient<WaitingPeer>,
            request: Request<SendHeaderRequest>,
            header: SignedHeader,
            retries: u32,
        ) -> (
            SignedHeader,
            Result<Response<SendHeaderResponse>, Status>,
            u32,
        ) {
            let backoff_multiplier = std::cmp::min(retries * 3 / 2, MAX_BACKOFF_MULTIPLIER);
            sleep(BACKOFF_INTERVAL * backoff_multiplier).await;
            let resp = client.send_header(request).await;
            (header, resp, retries)
        }

        loop {
            tokio::select! {
                result = rx_own_header_broadcast.recv(), if requests.len() < BROADCAST_CONCURRENCY => {
                    let header = match result {
                        Ok(header) => header,
                        Err(broadcast::error::RecvError::Closed) => {
                            trace!("Sender to {peer_authority} is shutting down!");
                            return;
                        }
                        Err(broadcast::error::RecvError::Lagged(e)) => {
                            warn!("Sender to {peer_authority} is lagging! {e}");
                            // Re-run the loop to receive again.
                            continue;
                        }
                    };
                    let request = Request::new(SendHeaderRequest { signed_header: header.clone() }).with_timeout(PUSH_TIMEOUT);
                    requests.push(send_header(client.clone(),request, header, 0));
                }
                Some((header, resp, retries)) = requests.next() => {
                    match resp {
                        Ok(_) => {},
                        Err(_) => {
                            // Retry broadcasting until the header is received.
                            // If a remote host was down for awhile, after restarting it will
                            // first receive the retried old headers, then the latest headers.
                            let request = Request::new(SendHeaderRequest { signed_header: header.clone() }).with_timeout(PUSH_TIMEOUT);
                            requests.push(send_header(client.clone(), request, header, retries + 1));
                        },
                    };
                }
            };
        }
    }
}

struct Inner {
    // The id of this primary.
    authority_id: AuthorityIdentifier,
    // Committee of the current epoch.
    committee: Committee,
    // Client for fetching payloads.
    client: NetworkClient,
    // Sender for broadcasting own headers.
    tx_own_header_broadcast: broadcast::Sender<SignedHeader>,
    // Background tasks proposing headers to peers.
    header_senders: Mutex<JoinSet<()>>,
}
