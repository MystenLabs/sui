// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{PeerHeights, StateSync, StateSyncMessage};
use anemo::{rpc::Status, Request, Response, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use sui_types::{
    base_types::ExecutionDigests,
    messages::{CertifiedTransaction, TransactionEffects},
    messages_checkpoint::{
        CertifiedCheckpointSummary as Checkpoint, CheckpointContents, CheckpointContentsDigest,
        CheckpointDigest, CheckpointSequenceNumber, VerifiedCheckpoint,
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

        self.peer_heights
            .write()
            .unwrap()
            .update_peer_height(peer_id, Some(checkpoint.clone()));

        let highest_verified_checkpoint = self
            .store
            .get_highest_verified_checkpoint()
            .map_err(|e| Status::internal(e.to_string()))?
            .map(|x| x.sequence_number());

        // If this checkpoint is higher than our highest verified checkpoint notify the
        // event loop to potentially sync it
        if Some(checkpoint.sequence_number()) > highest_verified_checkpoint {
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
            GetCheckpointSummaryRequest::Latest => self.store.get_highest_synced_checkpoint(),
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
    ) -> Result<Response<Option<CheckpointContents>>, Status> {
        let contents = self
            .store
            .get_checkpoint_contents(request.inner())
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(contents))
    }

    async fn get_transaction_and_effects(
        &self,
        request: Request<ExecutionDigests>,
    ) -> Result<Response<Option<(CertifiedTransaction, TransactionEffects)>>, Status> {
        let ExecutionDigests {
            transaction,
            effects,
        } = request.into_inner();

        let transaction = if let Some(transaction) = self
            .store
            .get_transaction(&transaction)
            .map_err(|e| Status::internal(e.to_string()))?
        {
            transaction
        } else {
            return Ok(Response::new(None));
        };

        let effects = if let Some(effects) = self
            .store
            .get_transaction_effects(&effects)
            .map_err(|e| Status::internal(e.to_string()))?
        {
            effects
        } else {
            return Ok(Response::new(None));
        };

        Ok(Response::new(Some((transaction.into_inner(), effects))))
    }
}
