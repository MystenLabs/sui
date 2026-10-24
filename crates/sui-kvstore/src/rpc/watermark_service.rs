// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `sui.kvstore.v1alpha.WatermarkService`: streams how far this instance's pipelines have durable
//! data rows to peer instances and readers. Per pipeline, that is the higher of its stored
//! watermark row read at startup and this instance's durable progress since
//! ([`crate::DurableProgress`]).

use std::collections::HashMap;
use std::pin::Pin;

use futures::Stream;
use sui_futures::service::Service;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tonic::Request;
use tonic::Response;
use tonic::Status;

use crate::WatermarkV1;
use crate::rpc::proto::v1alpha::PipelineWatermark;
use crate::rpc::proto::v1alpha::SubscribeWatermarksRequest;
use crate::rpc::proto::v1alpha::SubscribeWatermarksResponse;
use crate::rpc::proto::v1alpha::watermark_service_server;

pub struct WatermarkService {
    /// The relay's copy of the durable watermarks, which open streams read.
    progress: watch::Receiver<HashMap<String, CommitterWatermark>>,
    shutdown: CancellationToken,
}

impl WatermarkService {
    /// Serves, per pipeline, the higher of its row in `stored`, the stored watermark rows read at
    /// startup, and its entry in `progress`, this instance's durable watermarks. Including the
    /// stored rows lets a subscriber's first response give the height of every pipeline that has
    /// one, even a pipeline this instance has not advanced since it started (for example during a
    /// chain halt). Rows that readers treat as missing are left out. Open streams end when
    /// `shutdown` is cancelled or the returned relay stops.
    ///
    /// `watch` wakes every waiting receiver inside the call that updates the value, and the
    /// committers update `progress` on their commit path. So streams don't wait on `progress`
    /// themselves: the returned relay copies each update into a second channel that they read,
    /// and an update to `progress` wakes only the relay, however many streams are open. The relay
    /// runs until the sender behind `progress` is dropped, and is aborted on shutdown.
    pub fn new(
        stored: &HashMap<String, WatermarkV1>,
        mut progress: watch::Receiver<HashMap<String, CommitterWatermark>>,
        shutdown: CancellationToken,
    ) -> (Self, Service) {
        let mut served: HashMap<String, CommitterWatermark> = stored
            .iter()
            .filter_map(|(pipeline, row)| Some((pipeline.clone(), readable_watermark(row)?)))
            .collect();
        copy_advanced(&progress.borrow_and_update(), &mut served);

        let (relay_sender, relayed) = watch::channel(served);
        let relay = Service::new().spawn_aborting(relay_watermarks(progress, relay_sender));
        let service = Self {
            progress: relayed,
            shutdown,
        };
        (service, relay)
    }
}

/// The committer watermark that a stored watermark row records, or `None` if readers treat the
/// row as missing: it has no checkpoint, or its checkpoint is below `reader_lo`.
fn readable_watermark(row: &WatermarkV1) -> Option<CommitterWatermark> {
    let checkpoint_hi_inclusive = row
        .checkpoint_hi_inclusive
        .filter(|checkpoint| *checkpoint >= row.reader_lo)?;
    Some(CommitterWatermark {
        epoch_hi_inclusive: row.epoch_hi_inclusive,
        checkpoint_hi_inclusive,
        tx_hi: row.tx_hi,
        timestamp_ms_hi_inclusive: row.timestamp_ms_hi_inclusive,
    })
}

/// Copies every update of `progress` into `relayed` until the sender behind `progress` is
/// dropped.
async fn relay_watermarks(
    mut progress: watch::Receiver<HashMap<String, CommitterWatermark>>,
    relayed: watch::Sender<HashMap<String, CommitterWatermark>>,
) -> anyhow::Result<()> {
    while progress.changed().await.is_ok() {
        // `progress` is borrowed only inside the closure, so the committers can update it again
        // while `send_if_modified` wakes the streams.
        relayed.send_if_modified(|copy| copy_advanced(&progress.borrow_and_update(), copy));
    }
    Ok(())
}

/// Copies into `relayed` every entry of `progress` whose checkpoint is above `relayed`'s entry
/// for its pipeline. Returns whether `relayed` changed.
fn copy_advanced(
    progress: &HashMap<String, CommitterWatermark>,
    relayed: &mut HashMap<String, CommitterWatermark>,
) -> bool {
    let mut changed = false;
    for (pipeline, watermark) in progress {
        match relayed.get_mut(pipeline) {
            Some(known) if known.checkpoint_hi_inclusive >= watermark.checkpoint_hi_inclusive => {}
            Some(known) => {
                *known = *watermark;
                changed = true;
            }
            None => {
                relayed.insert(pipeline.clone(), *watermark);
                changed = true;
            }
        }
    }
    changed
}

#[tonic::async_trait]
impl watermark_service_server::WatermarkService for WatermarkService {
    type SubscribeWatermarksStream =
        Pin<Box<dyn Stream<Item = Result<SubscribeWatermarksResponse, Status>> + Send>>;

    async fn subscribe_watermarks(
        &self,
        _request: Request<SubscribeWatermarksRequest>,
    ) -> Result<Response<Self::SubscribeWatermarksStream>, Status> {
        let mut progress = self.progress.clone();
        let shutdown = self.shutdown.clone();

        let stream = async_stream::stream! {
            // The highest checkpoint sent to this subscriber, per pipeline.
            let mut sent: HashMap<String, u64> = HashMap::new();
            let mut first_response = true;

            loop {
                let watermarks = unsent_watermarks(&progress.borrow_and_update(), &mut sent);
                if first_response || !watermarks.is_empty() {
                    first_response = false;
                    yield Ok(SubscribeWatermarksResponse { watermarks });
                }

                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    changed = progress.changed() => {
                        if changed.is_err() {
                            break;
                        }
                    }
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }
}

/// Encodes a pipeline's committer watermark for the wire.
pub(crate) fn encode_watermark(
    pipeline: &str,
    watermark: &CommitterWatermark,
) -> PipelineWatermark {
    PipelineWatermark {
        pipeline: Some(pipeline.to_string()),
        epoch_hi_inclusive: Some(watermark.epoch_hi_inclusive),
        checkpoint_hi_inclusive: Some(watermark.checkpoint_hi_inclusive),
        tx_hi: Some(watermark.tx_hi),
        timestamp_ms_hi_inclusive: Some(watermark.timestamp_ms_hi_inclusive),
    }
}

/// Decodes a pipeline's committer watermark from the wire. Returns `None` if any field is
/// missing.
pub(crate) fn decode_watermark(
    watermark: PipelineWatermark,
) -> Option<(String, CommitterWatermark)> {
    Some((
        watermark.pipeline?,
        CommitterWatermark {
            epoch_hi_inclusive: watermark.epoch_hi_inclusive?,
            checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive?,
            tx_hi: watermark.tx_hi?,
            timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive?,
        },
    ))
}

/// The entries of `current` whose checkpoint is above what `sent` records for their pipeline,
/// encoded for the wire. Records them in `sent`.
fn unsent_watermarks(
    current: &HashMap<String, CommitterWatermark>,
    sent: &mut HashMap<String, u64>,
) -> Vec<PipelineWatermark> {
    let mut unsent = vec![];
    for (pipeline, watermark) in current {
        let checkpoint = watermark.checkpoint_hi_inclusive;
        if sent
            .get(pipeline)
            .is_none_or(|&sent_checkpoint| checkpoint > sent_checkpoint)
        {
            sent.insert(pipeline.clone(), checkpoint);
            unsent.push(encode_watermark(pipeline, watermark));
        }
    }
    unsent
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use watermark_service_server::WatermarkService as _;

    use super::*;

    fn stored_row(checkpoint: Option<u64>, reader_lo: u64) -> WatermarkV1 {
        WatermarkV1 {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: checkpoint,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: checkpoint.unwrap_or(0) * 1_000,
            reader_lo,
            pruner_hi: 0,
            pruner_timestamp_ms: 0,
            bucket_start_cp: None,
        }
    }

    fn watermark(checkpoint: u64) -> CommitterWatermark {
        CommitterWatermark {
            checkpoint_hi_inclusive: checkpoint,
            timestamp_ms_hi_inclusive: checkpoint * 1_000,
            ..Default::default()
        }
    }

    fn map<W>(entries: impl IntoIterator<Item = (&'static str, W)>) -> HashMap<String, W> {
        entries
            .into_iter()
            .map(|(pipeline, value)| (pipeline.to_string(), value))
            .collect()
    }

    /// The checkpoint `response` reports per pipeline.
    fn checkpoints(response: SubscribeWatermarksResponse) -> HashMap<String, u64> {
        response
            .watermarks
            .into_iter()
            .filter_map(decode_watermark)
            .map(|(pipeline, watermark)| (pipeline, watermark.checkpoint_hi_inclusive))
            .collect()
    }

    #[tokio::test]
    async fn first_response_starts_from_readable_stored_rows() {
        let stored = map([
            ("idle", stored_row(Some(10), 0)),
            ("advanced", stored_row(Some(10), 0)),
            ("behind_row", stored_row(Some(10), 0)),
            ("no_checkpoint", stored_row(None, 0)),
            ("below_reader_lo", stored_row(Some(10), 11)),
        ]);
        let (progress_sender, progress) = watch::channel(map([
            ("advanced", watermark(12)),
            ("behind_row", watermark(8)),
            ("unstored", watermark(3)),
        ]));
        let (service, _relay) = WatermarkService::new(&stored, progress, CancellationToken::new());
        let mut stream = service
            .subscribe_watermarks(Request::new(SubscribeWatermarksRequest {}))
            .await
            .unwrap()
            .into_inner();

        // A pipeline counts at the higher of its readable stored row and this instance's
        // progress; rows that readers treat as missing are left out.
        let first = checkpoints(stream.next().await.unwrap().unwrap());
        assert_eq!(
            first,
            map([
                ("idle", 10),
                ("advanced", 12),
                ("behind_row", 10),
                ("unstored", 3)
            ])
        );

        // Progress past a stored row is streamed; progress still below it is not.
        progress_sender.send_modify(|progress| {
            progress.insert("behind_row".to_string(), watermark(9));
            progress.insert("idle".to_string(), watermark(11));
        });
        let next = checkpoints(stream.next().await.unwrap().unwrap());
        assert_eq!(next, map([("idle", 11)]));
    }
}
