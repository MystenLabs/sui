// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use tokio::sync::broadcast;
use tonic::transport::Uri;

use crate::config::SubscriptionConfig;

use super::processed_checkpoint::ProcessedCheckpoint;

pub(crate) type CheckpointBroadcaster = broadcast::Sender<Arc<ProcessedCheckpoint>>;

/// Background service that connects to a fullnode's gRPC SubscribeCheckpoints endpoint,
/// processes incoming checkpoints, and broadcasts them to subscription resolvers.
pub(crate) struct CheckpointStreamTask {
    #[allow(dead_code)]
    uri: Uri,
    broadcaster: CheckpointBroadcaster,
}

impl CheckpointStreamTask {
    pub(crate) fn new(uri: Uri, config: &SubscriptionConfig) -> Self {
        let (broadcaster, _) = broadcast::channel(config.broadcast_buffer);
        Self { uri, broadcaster }
    }

    pub(crate) fn broadcaster(&self) -> CheckpointBroadcaster {
        self.broadcaster.clone()
    }
}
