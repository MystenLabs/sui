// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Follows the `WatermarkService` streams of kvstore instances and merges their durable
//! watermarks. A kvstore instance follows its peers to compare its own progress against them
//! ([`crate::readiness`]), and a reader follows the instances that write the data it serves to
//! learn how far that data goes.

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

/// How long to wait for a TCP connection to an instance.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long an attempt to subscribe to an instance may take, from connecting until its first
/// response, before it counts as failed.
const FIRST_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Time between HTTP/2 pings on a connection to an instance. An instance that disappears without
/// closing the connection (for example when its node fails) stops acknowledging them.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);

/// How long to wait for an instance to acknowledge a ping before the connection counts as failed
/// and the instance is subscribed to again.
const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(5);

/// Time between an instance's stream ending or failing and the next attempt to subscribe.
const RESUBSCRIBE_INTERVAL: Duration = Duration::from_secs(2);

/// What the followed instances have reported.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StreamedWatermarks {
    /// Instances that have neither delivered a first response nor failed a first attempt to
    /// subscribe. Each instance's first attempt is bounded by `CONNECT_TIMEOUT` and
    /// `FIRST_RESPONSE_TIMEOUT`, so this drops to zero shortly after startup.
    pub unsettled_instances: usize,

    /// Per pipeline, the durable watermark with the highest checkpoint any instance has reported.
    /// Entries only move forward and outlive the stream that reported them.
    pub watermarks: HashMap<String, CommitterWatermark>,
}

/// Subscribe to the `WatermarkService` of every kvstore instance in `urls`.
///
/// The returned service follows each instance, subscribing again after its stream ends or fails,
/// until every receiver is dropped. It is aborted on shutdown. Fails if a URL is not an `http://`
/// URL with a host.
pub fn follow(urls: &[String]) -> anyhow::Result<(watch::Receiver<StreamedWatermarks>, Service)> {
    let endpoints = urls
        .iter()
        .map(|url| {
            instance_endpoint(url).with_context(|| format!("Invalid kvstore instance URL {url:?}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let (sender, receiver) = watch::channel(StreamedWatermarks {
        unsettled_instances: endpoints.len(),
        watermarks: HashMap::new(),
    });

    let sender = Arc::new(sender);
    let mut service = Service::new();
    for endpoint in endpoints {
        service = service.spawn_aborting(follow_instance(endpoint, sender.clone()));
    }

    Ok((receiver, service))
}

/// The endpoint for the kvstore instance at `url`. Instances are reached over plaintext HTTP/2, so
/// `url` must be an `http://` URL with a host; anything else would fail on every attempt to
/// connect.
fn instance_endpoint(url: &str) -> anyhow::Result<Endpoint> {
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

async fn follow_instance(
    endpoint: Endpoint,
    sender: Arc<watch::Sender<StreamedWatermarks>>,
) -> anyhow::Result<()> {
    let url = endpoint.uri().to_string();
    let mut settled = false;
    let mut failing = false;

    loop {
        let result = tokio::select! {
            _ = sender.closed() => return Ok(()),
            result = subscribe_once(&endpoint, &sender, &mut settled, &mut failing) => result,
        };

        match result {
            Ok(()) => info!(url, "Watermark stream ended"),
            // Repeated failures (for example while an instance is restarting) are logged once.
            Err(e) if failing => debug!(url, "Failed to follow watermark stream: {e:#}"),
            Err(e) => {
                failing = true;
                warn!(url, "Failed to follow watermark stream: {e:#}");
            }
        }

        if !settled {
            settled = true;
            sender.send_modify(|state| state.unsettled_instances -= 1);
        }

        tokio::select! {
            _ = sender.closed() => return Ok(()),
            _ = tokio::time::sleep(RESUBSCRIBE_INTERVAL) => {}
        }
    }
}

/// Subscribe to the instance at `endpoint` and merge its responses into `sender` until the stream
/// ends. When the first response arrives, marks the instance `settled` and clears `failing`, so
/// that the next failure is logged as a new one.
async fn subscribe_once(
    endpoint: &Endpoint,
    sender: &watch::Sender<StreamedWatermarks>,
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
            state.unsettled_instances -= 1;
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

    /// Waits until the streamed watermarks satisfy `condition`, or panics after 5 seconds.
    async fn wait_for(
        streamed: &mut watch::Receiver<StreamedWatermarks>,
        condition: impl FnMut(&StreamedWatermarks) -> bool,
    ) {
        tokio::time::timeout(Duration::from_secs(5), streamed.wait_for(condition))
            .await
            .expect("timed out waiting for streamed watermarks")
            .expect("streamed watermark sender dropped");
    }

    fn checkpoint(streamed: &StreamedWatermarks, pipeline: &str) -> Option<u64> {
        streamed
            .watermarks
            .get(pipeline)
            .map(|watermark| watermark.checkpoint_hi_inclusive)
    }

    #[tokio::test]
    async fn keeps_the_highest_watermark_across_instances() {
        let progress_a = DurableProgress::new();
        progress_a.record("p", watermark(5));
        progress_a.record("q", watermark(1));
        let progress_b = DurableProgress::new();
        progress_b.record("p", watermark(3));
        progress_b.record("q", watermark(7));

        let (url_a, server_a) = serve(&progress_a).await;
        let (url_b, server_b) = serve(&progress_b).await;
        let (mut streamed, _follower) = follow(&[url_a, url_b]).unwrap();

        wait_for(&mut streamed, |state| {
            state.unsettled_instances == 0
                && checkpoint(state, "p") == Some(5)
                && checkpoint(state, "q") == Some(7)
        })
        .await;

        // Progress recorded after subscribing is streamed too.
        progress_b.record("p", watermark(9));
        wait_for(&mut streamed, |state| checkpoint(state, "p") == Some(9)).await;

        server_a.shutdown().await.unwrap();
        server_b.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn instance_without_progress_settles() {
        let progress = DurableProgress::new();
        let (url, server) = serve(&progress).await;
        let (mut streamed, _follower) = follow(&[url]).unwrap();

        wait_for(&mut streamed, |state| state.unsettled_instances == 0).await;
        assert!(streamed.borrow().watermarks.is_empty());

        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn unreachable_instance_settles() {
        let url = format!("http://127.0.0.1:{}", get_available_port());
        let (mut streamed, _follower) = follow(&[url]).unwrap();

        wait_for(&mut streamed, |state| state.unsettled_instances == 0).await;
        assert!(streamed.borrow().watermarks.is_empty());
    }

    #[tokio::test]
    async fn server_shutdown_ends_open_streams() {
        let progress = DurableProgress::new();
        progress.record("p", watermark(1));
        let (url, server) = serve(&progress).await;
        let (mut streamed, _follower) = follow(&[url]).unwrap();
        wait_for(&mut streamed, |state| checkpoint(state, "p") == Some(1)).await;

        tokio::time::timeout(Duration::from_secs(5), server.shutdown())
            .await
            .expect("server shutdown waited on an open watermark stream")
            .unwrap();
    }

    #[test]
    fn invalid_url_fails() {
        assert!(follow(&["not a url".to_string()]).is_err());
    }

    #[tokio::test]
    async fn url_without_scheme_fails() {
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
    async fn follower_leaves_an_unresponsive_instance() {
        let progress = DurableProgress::new();
        progress.record("p", watermark(1));
        let (upstream_url, _server) = serve(&progress).await;
        let (proxy_url, freeze) = freezing_proxy(&upstream_url).await;

        let (mut streamed, _follower) = follow(&[proxy_url]).unwrap();
        wait_for(&mut streamed, |state| checkpoint(state, "p") == Some(1)).await;

        freeze.cancel();
        progress.record("p", watermark(2));

        // Keepalive gives up on the frozen connection, and the follower subscribes again.
        tokio::time::timeout(
            KEEPALIVE_INTERVAL + KEEPALIVE_TIMEOUT + Duration::from_secs(20),
            streamed.wait_for(|state| checkpoint(state, "p") == Some(2)),
        )
        .await
        .expect("follower never left the unresponsive instance")
        .unwrap();
    }

    #[tokio::test]
    async fn server_shutdown_closes_an_unresponsive_subscriber() {
        let progress = DurableProgress::new();
        progress.record("p", watermark(1));
        let (upstream_url, server) = serve(&progress).await;
        let (proxy_url, freeze) = freezing_proxy(&upstream_url).await;

        let (mut streamed, follower) = follow(&[proxy_url]).unwrap();
        wait_for(&mut streamed, |state| checkpoint(state, "p") == Some(1)).await;

        freeze.cancel();
        drop(streamed);
        drop(follower);

        tokio::time::timeout(Duration::from_secs(40), server.shutdown())
            .await
            .expect("server shutdown waited on the unresponsive subscriber")
            .unwrap();
    }
}
