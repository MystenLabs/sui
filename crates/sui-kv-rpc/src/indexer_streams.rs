// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sets the checkpoint a replica serves from the `WatermarkService` streams of kvstore indexers,
//! without reading Bigtable's watermark rows.
//!
//! Every kvstore indexer writes the same Bigtable instance, and a durable watermark that any of
//! them reports for a pipeline means that pipeline's rows up to it are in the cluster that indexer
//! writes to. So each pipeline counts with the highest watermark any indexer has reported
//! ([`sui_kvstore::streamed_watermarks`] merges them), and the replica serves up to the lowest of
//! those across the pipelines it serves, the same bound it takes over Bigtable's watermark rows.
//! Merged watermarks only move forward, so neither does the served checkpoint.
//!
//! Bigtable replicates rows between clusters only later, so this bound holds only if the replica
//! reads from the cluster the indexers write to. Polling Bigtable's watermark rows needs no such
//! condition, because it reads them through the replica's own app profile.
//!
//! Bigtable's watermark rows also carry a `reader_lo`, and a pipeline whose watermark is below it
//! counts as missing. Streamed watermarks carry none, so this source cannot detect that case. The
//! kvstore does not prune, so the case does not arise; `GetServiceInfo` reports 0 as the lowest
//! available checkpoint for the same reason.
//!
//! An indexer's first response holds every pipeline it runs at the higher of its stored watermark
//! row, read when the indexer started, and its durable progress since. So a replica normally learns
//! its served checkpoint from the first response of one indexer, even during a chain halt. Until
//! every served pipeline has a watermark, requests that depend on the served checkpoint are
//! answered `Unavailable`, and [`crate::Health`] reports the replica not ready.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Context as _;
use sui_futures::service::Service;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use sui_kvstore::streamed_watermarks;
use sui_kvstore::streamed_watermarks::StreamedWatermarks;
use tokio::sync::watch;
use tokio::time::Instant;
use tracing::info;
use tracing::warn;

use crate::KvRpcServer;
use crate::v2;

/// Minimum time between log lines naming the served pipelines that no indexer has streamed a
/// watermark for.
const MISSING_PIPELINES_LOG_INTERVAL: Duration = Duration::from_secs(30);

/// Where a replica learns how far the data in Bigtable goes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum WatermarkSource {
    /// Poll the pipelines' watermark rows in Bigtable.
    #[default]
    Bigtable,

    /// Follow the `WatermarkService` streams of the kvstore indexers at these gRPC URLs. Bigtable's
    /// watermark rows are never read, even while every stream is down. The replica must read from
    /// the Bigtable cluster the indexers write to.
    IndexerStreams(Vec<String>),
}

/// Follow the indexers at `indexer_urls` and publish the service info their streams support to
/// `server`'s cache. Following stops when the returned service shuts down.
pub(crate) fn follow(server: &KvRpcServer, indexer_urls: &[String]) -> anyhow::Result<Service> {
    anyhow::ensure!(
        !indexer_urls.is_empty(),
        "following indexer watermark streams needs at least one indexer URL"
    );
    let (streamed, follower) = streamed_watermarks::follow(indexer_urls)
        .context("Failed to follow the indexers in indexer-urls")?;
    info!(
        ?indexer_urls,
        "Following kvstore indexer watermark streams; Bigtable watermark rows are not read"
    );

    let publisher = Service::new().spawn_aborting(publish_service_info(server.clone(), streamed));
    Ok(follower.merge(publisher))
}

/// Publish the service info that `streamed` supports to `server`'s cache whenever the served
/// checkpoint changes, until the sender behind `streamed` is dropped. The cache is never cleared,
/// so while every stream is down the replica keeps serving the last checkpoint they supported.
async fn publish_service_info(
    server: KvRpcServer,
    mut streamed: watch::Receiver<StreamedWatermarks>,
) -> anyhow::Result<()> {
    let pipelines = &server.service_info_watermark_pipelines;
    let mut served_checkpoint: Option<u64> = None;
    let mut missing_logged_at: Option<Instant> = None;

    loop {
        let (lowest, missing) = {
            let state = streamed.borrow_and_update();
            let lowest = lowest_watermark(&state.watermarks, pipelines).copied();
            // Until every indexer has answered or failed once, a missing pipeline can still arrive
            // in an indexer's first response.
            let missing: Vec<&str> = if lowest.is_none() && state.unsettled_instances == 0 {
                pipelines
                    .iter()
                    .copied()
                    .filter(|pipeline| !state.watermarks.contains_key(*pipeline))
                    .collect()
            } else {
                vec![]
            };
            (lowest, missing)
        };

        match lowest {
            // Most updates advance a pipeline that does not bound the served checkpoint. Publishing
            // them would wake every monotonic read waiting on the cache without changing its
            // answer. A checkpoint's epoch and timestamp are fixed, so the checkpoint alone tells
            // whether the answer changed.
            Some(watermark) if served_checkpoint != Some(watermark.checkpoint_hi_inclusive) => {
                if served_checkpoint.is_none() {
                    info!(
                        checkpoint = watermark.checkpoint_hi_inclusive,
                        "Every served pipeline has a streamed watermark; the served checkpoint is \
                         available",
                    );
                }
                served_checkpoint = Some(watermark.checkpoint_hi_inclusive);
                let info = v2::service_info(
                    server.chain_id,
                    server.server_version.as_ref(),
                    watermark.epoch_hi_inclusive,
                    watermark.checkpoint_hi_inclusive,
                    watermark.timestamp_ms_hi_inclusive,
                );
                server.metrics.observe_served_service_info(&info);
                server.cache.send_replace(Some(info));
            }
            Some(_) => {}
            None if !missing.is_empty()
                && missing_logged_at
                    .is_none_or(|logged| logged.elapsed() >= MISSING_PIPELINES_LOG_INTERVAL) =>
            {
                warn!(
                    ?missing,
                    "No indexer has streamed a watermark for these served pipelines; requests \
                     that depend on the served checkpoint are answered Unavailable until one does",
                );
                missing_logged_at = Some(Instant::now());
            }
            None => {}
        }

        if streamed.changed().await.is_err() {
            return Ok(());
        }
    }
}

/// The watermark with the lowest checkpoint among those of `pipelines`, or `None` while any of
/// `pipelines` has no watermark in `watermarks`.
fn lowest_watermark<'w>(
    watermarks: &'w HashMap<String, CommitterWatermark>,
    pipelines: &[&str],
) -> Option<&'w CommitterWatermark> {
    let mut lowest: Option<&CommitterWatermark> = None;
    for &pipeline in pipelines {
        let watermark = watermarks.get(pipeline)?;
        if lowest
            .is_none_or(|lowest| watermark.checkpoint_hi_inclusive < lowest.checkpoint_hi_inclusive)
        {
            lowest = Some(watermark);
        }
    }
    lowest
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::time::Duration;

    use sui_kvstore::BigTableClient;
    use sui_kvstore::EPOCH_END_PIPELINE;
    use sui_kvstore::OBJECTS_PIPELINE;
    use sui_kvstore::WatermarkV1;
    use sui_kvstore::rpc::RpcService;
    use sui_kvstore::rpc::proto::v1alpha::watermark_service_server::WatermarkServiceServer;
    use sui_kvstore::rpc::watermark_service::WatermarkService;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_rpc::proto::sui::rpc::v2::GetServiceInfoRequest;
    use sui_rpc::proto::sui::rpc::v2::GetServiceInfoResponse;
    use sui_rpc::proto::sui::rpc::v2::ledger_service_server::LedgerService;
    use sui_rpc_api::proto::timestamp_ms_to_proto;
    use tokio::time::sleep;
    use tokio::time::timeout;

    use super::*;
    use crate::DEFAULT_SERVICE_INFO_WATERMARK_PIPELINES;
    use crate::KvRpcConfig;
    use crate::LedgerHistoryConfig;
    use crate::ServerConfig;
    use crate::StagesConfig;
    use crate::tests::watermark_reads;

    /// Covers subscribing to an indexer that restarted, which waits out the follower's 2 s
    /// resubscribe interval.
    const WAIT: Duration = Duration::from_secs(10);

    /// Long enough for a 10 ms Bigtable polling loop, if one ran, to read the watermark rows many
    /// times, and for a streamed update to reach the replica.
    const QUIET_PERIOD: Duration = Duration::from_millis(300);

    fn watermark(checkpoint: u64) -> CommitterWatermark {
        CommitterWatermark {
            epoch_hi_inclusive: checkpoint / 10,
            checkpoint_hi_inclusive: checkpoint,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: checkpoint * 1_000,
        }
    }

    /// A watermark for every pipeline a replica without the List APIs serves, at the checkpoint
    /// `checkpoint` picks for it.
    fn served_watermarks(checkpoint: impl Fn(&str) -> u64) -> HashMap<String, CommitterWatermark> {
        DEFAULT_SERVICE_INFO_WATERMARK_PIPELINES
            .iter()
            .map(|&pipeline| (pipeline.to_string(), watermark(checkpoint(pipeline))))
            .collect()
    }

    /// A free local address.
    fn local_address() -> SocketAddr {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
    }

    /// A kvstore indexer's `WatermarkService`, streaming the durable progress in `progress`.
    struct Indexer {
        url: String,
        progress: watch::Sender<HashMap<String, CommitterWatermark>>,
        server: Service,
    }

    impl Indexer {
        async fn start(address: SocketAddr, progress: HashMap<String, CommitterWatermark>) -> Self {
            let progress = watch::Sender::new(progress);
            let rpc = RpcService::new(address);
            let (watermarks, relay) =
                WatermarkService::new(&HashMap::new(), progress.subscribe(), rpc.shutdown_token());
            let server = rpc
                .add_service(WatermarkServiceServer::new(watermarks))
                .run(std::future::ready(()))
                .await
                .unwrap()
                .attach(relay);
            Self {
                url: format!("http://{address}"),
                progress,
                server,
            }
        }

        fn record(&self, progress: HashMap<String, CommitterWatermark>) {
            self.progress
                .send_modify(|current| current.extend(progress));
        }
    }

    /// A replica reading the mock Bigtable at `bigtable` that follows the indexers at
    /// `indexer_urls`, with its gRPC service started.
    async fn start_replica(
        bigtable: SocketAddr,
        indexer_urls: Vec<String>,
    ) -> (KvRpcServer, Service) {
        let server = KvRpcServer::new_local_with_config(
            bigtable.to_string(),
            "test".to_string(),
            None,
            LedgerHistoryConfig::default(),
            KvRpcConfig::default().request_bigtable_concurrency(),
            KvRpcConfig::default().monotonic_read_wait_timeout(),
            StagesConfig::default(),
            false,
            WatermarkSource::IndexerStreams(indexer_urls),
            false,
        )
        .await
        .expect("create kv-rpc server");
        let service = server
            .clone()
            .start_service(local_address(), ServerConfig::default())
            .await
            .expect("start kv-rpc server");
        (server, service)
    }

    /// Write a watermark row at `checkpoint` for every served pipeline to the mock Bigtable at
    /// `bigtable`.
    async fn write_bigtable_watermarks(bigtable: SocketAddr, checkpoint: u64) {
        let mut client = BigTableClient::new_local(bigtable.to_string(), "test".to_string())
            .await
            .expect("connect to mock Bigtable");
        for pipeline in DEFAULT_SERVICE_INFO_WATERMARK_PIPELINES {
            client
                .create_pipeline_watermark_if_absent(
                    pipeline,
                    &WatermarkV1 {
                        epoch_hi_inclusive: checkpoint / 10,
                        checkpoint_hi_inclusive: Some(checkpoint),
                        tx_hi: 0,
                        timestamp_ms_hi_inclusive: checkpoint * 1_000,
                        reader_lo: 0,
                        pruner_hi: 0,
                        pruner_timestamp_ms: 0,
                        bucket_start_cp: None,
                    },
                )
                .await
                .expect("write watermark row");
        }
    }

    async fn wait_for_served_checkpoint(
        server: &KvRpcServer,
        checkpoint: u64,
    ) -> GetServiceInfoResponse {
        let mut cache = server.cache.subscribe();
        let info = timeout(
            WAIT,
            cache.wait_for(|info| {
                info.as_ref().and_then(|info| info.checkpoint_height) == Some(checkpoint)
            }),
        )
        .await
        .unwrap_or_else(|_| panic!("served checkpoint never reached {checkpoint}"))
        .expect("service info sender dropped");
        info.clone().unwrap()
    }

    fn cached_checkpoint(server: &KvRpcServer) -> Option<u64> {
        server
            .cache
            .borrow()
            .as_ref()
            .and_then(|info| info.checkpoint_height)
    }

    async fn get_service_info(
        server: &KvRpcServer,
    ) -> Result<GetServiceInfoResponse, tonic::Status> {
        LedgerService::get_service_info(
            server,
            tonic::Request::new(GetServiceInfoRequest::default()),
        )
        .await
        .map(tonic::Response::into_inner)
    }

    #[test]
    fn lowest_watermark_needs_every_pipeline_and_ignores_others() {
        let mut watermarks = HashMap::from([
            ("a".to_string(), watermark(10)),
            ("b".to_string(), watermark(7)),
            ("unserved".to_string(), watermark(3)),
        ]);
        assert_eq!(lowest_watermark(&watermarks, &["a", "b", "c"]), None);

        watermarks.insert("c".to_string(), watermark(9));
        assert_eq!(
            lowest_watermark(&watermarks, &["a", "b", "c"]),
            Some(&watermark(7))
        );
    }

    #[tokio::test]
    async fn serves_the_lowest_of_each_pipelines_highest_watermark_across_indexers() {
        let mock = MockBigtableServer::new();
        let (bigtable, _mock_server) = mock.start().await.expect("start mock Bigtable");

        // Each indexer leads on different pipelines. Per pipeline, the higher watermark counts:
        // objects at 30 and every other pipeline at 28, so the replica serves 28. The lower of the
        // two indexers' own lowest watermarks would be 10, and the higher 12.
        let a = Indexer::start(
            local_address(),
            served_watermarks(|pipeline| if pipeline == OBJECTS_PIPELINE { 30 } else { 10 }),
        )
        .await;
        let b = Indexer::start(
            local_address(),
            served_watermarks(|pipeline| if pipeline == OBJECTS_PIPELINE { 12 } else { 28 }),
        )
        .await;
        let (server, _replica) = start_replica(bigtable, vec![a.url.clone(), b.url.clone()]).await;

        let info = wait_for_served_checkpoint(&server, 28).await;
        assert_eq!(info.epoch, Some(2));
        assert_eq!(info.timestamp, Some(timestamp_ms_to_proto(28_000)));
        assert_eq!(info.lowest_available_checkpoint, Some(0));
        assert_eq!(server.metrics.served_checkpoint.get(), 28);
        assert_eq!(server.metrics.served_checkpoint_timestamp_ms.get(), 28_000);

        // Indexer `a` passes `b` on every pipeline but objects, so objects bounds the served
        // checkpoint, at `a`'s 30 rather than `b`'s 12.
        a.record(served_watermarks(|pipeline| {
            if pipeline == OBJECTS_PIPELINE { 30 } else { 35 }
        }));
        let info = wait_for_served_checkpoint(&server, 30).await;
        assert_eq!(info.epoch, Some(3));
        assert_eq!(info.timestamp, Some(timestamp_ms_to_proto(30_000)));
        assert_eq!(server.metrics.served_checkpoint.get(), 30);
        assert_eq!(server.metrics.served_checkpoint_timestamp_ms.get(), 30_000);

        assert_eq!(watermark_reads(&mock).await, 0);
    }

    #[tokio::test]
    async fn never_falls_back_to_bigtable_watermark_rows() {
        let mock = MockBigtableServer::new();
        let (bigtable, _mock_server) = mock.start().await.expect("start mock Bigtable");
        // Bigtable's watermark rows alone would let the replica serve checkpoint 50.
        write_bigtable_watermarks(bigtable, 50).await;
        mock.clear_read_rows_calls().await;

        // The only indexer has not advanced the epoch end pipeline since it started.
        let address = local_address();
        let mut progress = served_watermarks(|_| 20);
        progress.remove(EPOCH_END_PIPELINE);
        let indexer = Indexer::start(address, progress).await;
        let (server, _replica) = start_replica(bigtable, vec![indexer.url.clone()]).await;

        sleep(QUIET_PERIOD).await;
        assert_eq!(cached_checkpoint(&server), None);
        let error = tonic::Status::from(server.served_height().unwrap_err());
        assert_eq!(error.code(), tonic::Code::Unavailable);
        let error = get_service_info(&server).await.unwrap_err();
        assert_eq!(error.code(), tonic::Code::Unavailable);
        assert_eq!(server.metrics.served_checkpoint.get(), 0);
        assert!(!server.health().is_ready());

        indexer.record(served_watermarks(|_| 20));
        wait_for_served_checkpoint(&server, 20).await;
        assert!(server.health().is_ready());

        // With every stream down, the replica keeps serving the last streamed checkpoint.
        indexer.server.shutdown().await.unwrap();
        sleep(QUIET_PERIOD).await;
        let info = get_service_info(&server).await.unwrap();
        assert_eq!(info.checkpoint_height, Some(20));
        assert_eq!(server.metrics.served_checkpoint.get(), 20);
        assert_eq!(watermark_reads(&mock).await, 0);
        assert!(server.health().is_ready());

        // Once the indexer is back, its stream advances the served checkpoint again.
        let _indexer = Indexer::start(address, served_watermarks(|_| 30)).await;
        wait_for_served_checkpoint(&server, 30).await;
        assert_eq!(watermark_reads(&mock).await, 0);
    }

    #[tokio::test]
    async fn shutting_down_the_replica_stops_following_indexers() {
        let mock = MockBigtableServer::new();
        let (bigtable, _mock_server) = mock.start().await.expect("start mock Bigtable");
        let indexer = Indexer::start(local_address(), served_watermarks(|_| 20)).await;
        let (server, replica) = start_replica(bigtable, vec![indexer.url.clone()]).await;
        wait_for_served_checkpoint(&server, 20).await;

        timeout(WAIT, replica.shutdown())
            .await
            .expect("kv-rpc shutdown did not finish")
            .unwrap();

        indexer.record(served_watermarks(|_| 30));
        sleep(QUIET_PERIOD).await;
        assert_eq!(cached_checkpoint(&server), Some(20));
    }
}
