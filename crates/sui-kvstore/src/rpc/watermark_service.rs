// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `sui.kvstore.v1alpha.WatermarkService`: streams this instance's durable watermarks
//! ([`crate::DurableProgress`]) to peer instances and readers.

use std::collections::HashMap;
use std::pin::Pin;

use futures::Stream;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tonic::Request;
use tonic::Response;
use tonic::Status;

use crate::rpc::proto::v1alpha::PipelineWatermark;
use crate::rpc::proto::v1alpha::SubscribeWatermarksRequest;
use crate::rpc::proto::v1alpha::SubscribeWatermarksResponse;
use crate::rpc::proto::v1alpha::watermark_service_server;

pub struct WatermarkService {
    progress: watch::Receiver<HashMap<String, CommitterWatermark>>,
    shutdown: CancellationToken,
}

impl WatermarkService {
    /// Serves `progress`, this instance's durable watermarks per pipeline. Open streams end when
    /// `shutdown` is cancelled or the sender behind `progress` is dropped.
    pub fn new(
        progress: watch::Receiver<HashMap<String, CommitterWatermark>>,
        shutdown: CancellationToken,
    ) -> Self {
        Self { progress, shutdown }
    }
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
