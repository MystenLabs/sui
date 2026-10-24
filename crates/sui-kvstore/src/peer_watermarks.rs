// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Follows peer kvstore instances' `WatermarkService` streams and merges their durable
//! watermarks, for readiness to compare this instance's progress against.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use sui_futures::service::Service;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use tokio::sync::watch;
use tonic::transport::Endpoint;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::rpc::proto::v1alpha::SubscribeWatermarksRequest;
use crate::rpc::proto::v1alpha::SubscribeWatermarksResponse;
use crate::rpc::proto::v1alpha::watermark_service_client::WatermarkServiceClient;
use crate::rpc::watermark_service::decode_watermark;

/// How long to wait for a TCP connection to a peer.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long an attempt to subscribe to a peer may take, from connecting until its first response,
/// before it counts as failed.
const FIRST_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Time between HTTP/2 pings on a connection to a peer. A peer that disappears without closing
/// the connection (for example when its node fails) stops acknowledging them.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);

/// How long to wait for a peer to acknowledge a ping before the connection counts as failed and
/// the peer is subscribed to again.
const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(5);

/// Time between a peer's stream ending or failing and the next attempt to subscribe.
const RESUBSCRIBE_INTERVAL: Duration = Duration::from_secs(2);

/// What this instance has heard from its peers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PeerWatermarks {
    /// Peers that have neither delivered a first response nor failed a first attempt to
    /// subscribe. Each peer's first attempt is bounded by `CONNECT_TIMEOUT` and
    /// `FIRST_RESPONSE_TIMEOUT`, so this drops to zero shortly after startup.
    pub unsettled_peers: usize,

    /// Per pipeline, the durable watermark with the highest checkpoint any peer has reported.
    /// Entries only move forward and outlive the stream that reported them.
    pub watermarks: HashMap<String, CommitterWatermark>,
}

/// Subscribe to the `WatermarkService` of every peer in `peer_urls`.
///
/// The returned service follows each peer, subscribing again after its stream ends or fails,
/// until every receiver is dropped. It is aborted on shutdown. Fails if a URL is not an `http://`
/// URL with a host.
pub fn follow(peer_urls: &[String]) -> anyhow::Result<(watch::Receiver<PeerWatermarks>, Service)> {
    let endpoints = peer_urls
        .iter()
        .map(|url| peer_endpoint(url).with_context(|| format!("Invalid peer URL {url:?}")))
        .collect::<anyhow::Result<Vec<_>>>()?;

    let (sender, receiver) = watch::channel(PeerWatermarks {
        unsettled_peers: endpoints.len(),
        watermarks: HashMap::new(),
    });

    let sender = Arc::new(sender);
    let mut service = Service::new();
    for endpoint in endpoints {
        service = service.spawn_aborting(follow_peer(endpoint, sender.clone()));
    }

    Ok((receiver, service))
}

/// The endpoint for the peer at `url`. Peers are reached over plaintext HTTP/2, so `url` must be an
/// `http://` URL with a host; anything else would fail on every attempt to connect.
fn peer_endpoint(url: &str) -> anyhow::Result<Endpoint> {
    let endpoint = Endpoint::from_shared(url.to_string())?;
    let uri = endpoint.uri();
    anyhow::ensure!(
        uri.scheme_str() == Some("http") && uri.host().is_some(),
        "expected an http:// URL with a host"
    );

    // With only a response stream open, hyper treats the connection as idle and sends no pings
    // unless pings while idle are enabled.
    Ok(endpoint
        .connect_timeout(CONNECT_TIMEOUT)
        .http2_keep_alive_interval(KEEPALIVE_INTERVAL)
        .keep_alive_timeout(KEEPALIVE_TIMEOUT)
        .keep_alive_while_idle(true))
}

async fn follow_peer(
    endpoint: Endpoint,
    sender: Arc<watch::Sender<PeerWatermarks>>,
) -> anyhow::Result<()> {
    let peer = endpoint.uri().to_string();
    let mut settled = false;
    let mut failing = false;

    loop {
        let result = tokio::select! {
            _ = sender.closed() => return Ok(()),
            result = subscribe_once(&endpoint, &sender, &mut settled, &mut failing) => result,
        };

        match result {
            Ok(()) => info!(peer, "Peer watermark stream ended"),
            // Repeated failures (for example while a peer is restarting) are logged once.
            Err(e) if failing => debug!(peer, "Failed to follow peer watermarks: {e:#}"),
            Err(e) => {
                failing = true;
                warn!(peer, "Failed to follow peer watermarks: {e:#}");
            }
        }

        if !settled {
            settled = true;
            sender.send_modify(|state| state.unsettled_peers -= 1);
        }

        tokio::select! {
            _ = sender.closed() => return Ok(()),
            _ = tokio::time::sleep(RESUBSCRIBE_INTERVAL) => {}
        }
    }
}

/// Subscribe to the peer at `endpoint` and merge its responses into `sender` until the stream
/// ends. When the first response arrives, marks the peer `settled` and clears `failing`, so that
/// the next failure is logged as a new one.
async fn subscribe_once(
    endpoint: &Endpoint,
    sender: &watch::Sender<PeerWatermarks>,
    settled: &mut bool,
    failing: &mut bool,
) -> anyhow::Result<()> {
    let (mut stream, first_response) = tokio::time::timeout(FIRST_RESPONSE_TIMEOUT, async {
        let channel = endpoint.connect().await.context("Failed to connect")?;
        let mut stream = WatermarkServiceClient::new(channel)
            .subscribe_watermarks(SubscribeWatermarksRequest {})
            .await
            .context("Failed to subscribe")?
            .into_inner();
        let first_response = stream
            .message()
            .await
            .context("Failed to receive the first response")?
            .context("Stream ended before the first response")?;
        anyhow::Ok((stream, first_response))
    })
    .await
    .context("Timed out waiting for the first response")??;

    *failing = false;
    let newly_settled = !std::mem::replace(settled, true);
    sender.send_if_modified(|state| {
        let merged = merge_forward_only(&mut state.watermarks, first_response);
        if newly_settled {
            state.unsettled_peers -= 1;
        }
        merged || newly_settled
    });

    while let Some(response) = stream.message().await.context("Stream failed")? {
        sender.send_if_modified(|state| merge_forward_only(&mut state.watermarks, response));
    }

    Ok(())
}

/// Merge the watermarks in `response` into `watermarks`, keeping the highest checkpoint per
/// pipeline. Watermarks with missing fields are skipped. Returns whether `watermarks` changed.
fn merge_forward_only(
    watermarks: &mut HashMap<String, CommitterWatermark>,
    response: SubscribeWatermarksResponse,
) -> bool {
    let mut changed = false;
    for (pipeline, watermark) in response.watermarks.into_iter().filter_map(decode_watermark) {
        if watermarks
            .get(&pipeline)
            .is_none_or(|known| watermark.checkpoint_hi_inclusive > known.checkpoint_hi_inclusive)
        {
            watermarks.insert(pipeline, watermark);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use tokio::net::TcpListener;
    use tokio::net::TcpStream;
    use tokio_util::sync::CancellationToken;

    use crate::DurableProgress;
    use crate::rpc::RpcService;
    use crate::rpc::proto::v1alpha::watermark_service_server::WatermarkServiceServer;
    use crate::rpc::watermark_service::WatermarkService;
    use crate::testing::get_available_port;

    use super::*;

    fn watermark(checkpoint: u64) -> CommitterWatermark {
        CommitterWatermark {
            checkpoint_hi_inclusive: checkpoint,
            timestamp_ms_hi_inclusive: checkpoint * 1_000,
            ..Default::default()
        }
    }

    /// Serve `progress` through a `WatermarkService` on a free local port. Returns the server's
    /// URL and service.
    async fn serve(progress: &DurableProgress) -> (String, Service) {
        let rpc_listen_address = SocketAddr::from(([127, 0, 0, 1], get_available_port()));
        let rpc = RpcService::new(rpc_listen_address);
        let (watermarks, relay) =
            WatermarkService::new(&HashMap::new(), progress.subscribe(), rpc.shutdown_token());
        let service = rpc
            .add_service(WatermarkServiceServer::new(watermarks))
            .run(std::future::pending())
            .await
            .unwrap()
            .attach(relay);
        (format!("http://{rpc_listen_address}"), service)
    }

    /// Waits until the peer state satisfies `condition`, or panics after 5 seconds.
    async fn wait_for(
        peers: &mut watch::Receiver<PeerWatermarks>,
        condition: impl FnMut(&PeerWatermarks) -> bool,
    ) {
        tokio::time::timeout(Duration::from_secs(5), peers.wait_for(condition))
            .await
            .expect("timed out waiting for peer watermarks")
            .expect("peer watermark sender dropped");
    }

    fn checkpoint(peers: &PeerWatermarks, pipeline: &str) -> Option<u64> {
        peers
            .watermarks
            .get(pipeline)
            .map(|watermark| watermark.checkpoint_hi_inclusive)
    }

    #[tokio::test]
    async fn keeps_the_highest_watermark_across_peers() {
        let progress_a = DurableProgress::new();
        progress_a.record("p", watermark(5));
        progress_a.record("q", watermark(1));
        let progress_b = DurableProgress::new();
        progress_b.record("p", watermark(3));
        progress_b.record("q", watermark(7));

        let (url_a, server_a) = serve(&progress_a).await;
        let (url_b, server_b) = serve(&progress_b).await;
        let (mut peers, _follower) = follow(&[url_a, url_b]).unwrap();

        wait_for(&mut peers, |state| {
            state.unsettled_peers == 0
                && checkpoint(state, "p") == Some(5)
                && checkpoint(state, "q") == Some(7)
        })
        .await;

        // Progress recorded after subscribing is streamed too.
        progress_b.record("p", watermark(9));
        wait_for(&mut peers, |state| checkpoint(state, "p") == Some(9)).await;

        server_a.shutdown().await.unwrap();
        server_b.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn peer_without_progress_settles() {
        let progress = DurableProgress::new();
        let (url, server) = serve(&progress).await;
        let (mut peers, _follower) = follow(&[url]).unwrap();

        wait_for(&mut peers, |state| state.unsettled_peers == 0).await;
        assert!(peers.borrow().watermarks.is_empty());

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn unreachable_peer_settles() {
        let url = format!("http://127.0.0.1:{}", get_available_port());
        let (mut peers, _follower) = follow(&[url]).unwrap();

        wait_for(&mut peers, |state| state.unsettled_peers == 0).await;
        assert!(peers.borrow().watermarks.is_empty());
    }

    #[tokio::test]
    async fn server_shutdown_ends_open_streams() {
        let progress = DurableProgress::new();
        progress.record("p", watermark(1));
        let (url, server) = serve(&progress).await;
        let (mut peers, _follower) = follow(&[url]).unwrap();
        wait_for(&mut peers, |state| checkpoint(state, "p") == Some(1)).await;

        tokio::time::timeout(Duration::from_secs(5), server.shutdown())
            .await
            .expect("server shutdown waited on an open watermark stream")
            .unwrap();
    }

    #[test]
    fn invalid_peer_url_fails() {
        assert!(follow(&["not a url".to_string()]).is_err());
    }

    #[tokio::test]
    async fn peer_url_without_scheme_fails() {
        assert!(follow(&["kvstore-1:7001".to_string()]).is_err());
    }

    /// Forward a local port to `upstream`, and return its URL. Cancelling the returned token
    /// freezes the first connection, as if the machine at either end had failed: its sockets stay
    /// open but nothing more is forwarded. Later connections are forwarded normally.
    async fn freezing_proxy(upstream_url: &str) -> (String, CancellationToken) {
        let upstream = upstream_url.trim_start_matches("http://").to_string();
        let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_url = format!("http://{}", proxy.local_addr().unwrap());
        let freeze_first = CancellationToken::new();
        let freeze = freeze_first.clone();
        tokio::spawn(async move {
            let mut first = true;
            loop {
                let (mut client, _) = proxy.accept().await.unwrap();
                let Ok(mut server) = TcpStream::connect(&upstream).await else {
                    continue;
                };
                let freeze = if std::mem::replace(&mut first, false) {
                    freeze.clone()
                } else {
                    CancellationToken::new()
                };
                tokio::spawn(async move {
                    tokio::select! {
                        _ = tokio::io::copy_bidirectional(&mut client, &mut server) => {}
                        _ = freeze.cancelled() => std::future::pending::<()>().await,
                    }
                    drop((client, server));
                });
            }
        });
        (proxy_url, freeze_first)
    }

    #[tokio::test]
    async fn follower_leaves_an_unresponsive_peer() {
        let progress = DurableProgress::new();
        progress.record("p", watermark(1));
        let (upstream_url, _server) = serve(&progress).await;
        let (proxy_url, freeze) = freezing_proxy(&upstream_url).await;

        let (mut peers, _follower) = follow(&[proxy_url]).unwrap();
        wait_for(&mut peers, |state| checkpoint(state, "p") == Some(1)).await;

        freeze.cancel();
        progress.record("p", watermark(2));

        // Keepalive gives up on the frozen connection, and the follower subscribes again.
        tokio::time::timeout(
            KEEPALIVE_INTERVAL + KEEPALIVE_TIMEOUT + Duration::from_secs(20),
            peers.wait_for(|state| checkpoint(state, "p") == Some(2)),
        )
        .await
        .expect("follower never left the unresponsive peer")
        .unwrap();
    }

    #[tokio::test]
    async fn server_shutdown_closes_an_unresponsive_subscriber() {
        let progress = DurableProgress::new();
        progress.record("p", watermark(1));
        let (upstream_url, server) = serve(&progress).await;
        let (proxy_url, freeze) = freezing_proxy(&upstream_url).await;

        let (mut peers, follower) = follow(&[proxy_url]).unwrap();
        wait_for(&mut peers, |state| checkpoint(state, "p") == Some(1)).await;

        freeze.cancel();
        drop(peers);
        drop(follower);

        tokio::time::timeout(Duration::from_secs(40), server.shutdown())
            .await
            .expect("server shutdown waited on the unresponsive subscriber")
            .unwrap();
    }
}
