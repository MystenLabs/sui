// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{PeerHeights, StateSync, StateSyncMessage};
use anemo::{rpc::Status, Request, Response, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use sui_types::{
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::{
        CertifiedCheckpointSummary as Checkpoint, CheckpointSequenceNumber, FullCheckpointContents,
        VerifiedCheckpoint,
    },
    storage::ReadStore,
    storage::WriteStore,
};
use tokio::sync::mpsc;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GetCheckpointSummaryRequest {
    Latest,
    ByDigest(CheckpointDigest),
    BySequenceNumber(CheckpointSequenceNumber),
}

pub(super) struct Server<S> {
    pub(super) store: S,
    pub(super) peer_heights: Arc<RwLock<PeerHeights>>,
    pub(super) sender: mpsc::WeakSender<StateSyncMessage>,
}

#[anemo::async_trait]
impl<S> StateSync for Server<S>
where
    S: WriteStore + Send + Sync + 'static,
    <S as ReadStore>::Error: std::error::Error,
{
    async fn push_checkpoint_summary(
        &self,
        request: Request<Checkpoint>,
    ) -> Result<Response<()>, Status> {
        let peer_id = request
            .peer_id()
            .copied()
            .ok_or_else(|| Status::internal("unable to query sender's PeerId"))?;

        let checkpoint = request.into_inner();

        if !self
            .peer_heights
            .write()
            .unwrap()
            .update_peer_info(peer_id, checkpoint.clone())
        {
            return Ok(Response::new(()));
        }

        let highest_verified_checkpoint = *self
            .store
            .get_highest_verified_checkpoint()
            .map_err(|e| Status::internal(e.to_string()))?
            .sequence_number();

        // If this checkpoint is higher than our highest verified checkpoint notify the
        // event loop to potentially sync it
        if *checkpoint.sequence_number() > highest_verified_checkpoint {
            if let Some(sender) = self.sender.upgrade() {
                sender.send(StateSyncMessage::StartSyncJob).await.unwrap();
            }
        }

        Ok(Response::new(()))
    }

    async fn get_checkpoint_summary(
        &self,
        request: Request<GetCheckpointSummaryRequest>,
    ) -> Result<Response<Option<Checkpoint>>, Status> {
        let checkpoint = match request.inner() {
            GetCheckpointSummaryRequest::Latest => {
                self.store.get_highest_synced_checkpoint().map(Some)
            }
            GetCheckpointSummaryRequest::ByDigest(digest) => {
                self.store.get_checkpoint_by_digest(digest)
            }
            GetCheckpointSummaryRequest::BySequenceNumber(sequence_number) => self
                .store
                .get_checkpoint_by_sequence_number(*sequence_number),
        }
        .map_err(|e| Status::internal(e.to_string()))?
        .map(VerifiedCheckpoint::into_inner);

        Ok(Response::new(checkpoint))
    }

    async fn get_checkpoint_contents(
        &self,
        request: Request<CheckpointContentsDigest>,
    ) -> Result<Response<Option<FullCheckpointContents>>, Status> {
        let contents = self
            .store
            .get_full_checkpoint_contents(request.inner())
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(contents))
    }
}
