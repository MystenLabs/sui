// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{Randomness, RandomnessMessage, SendPartialSignaturesRequest};
use anemo::{Request, Response};
use std::sync::{Arc, RwLock};
use sui_types::{
    committee::EpochId,
    crypto::{RandomnessPartialSignature, RandomnessRound},
};
use tokio::sync::mpsc;

pub(super) struct Server {
    pub(super) sender: mpsc::WeakSender<RandomnessMessage>,
}

#[anemo::async_trait]
impl Randomness for Server {
    async fn send_partial_signatures(
        &self,
        request: Request<SendPartialSignaturesRequest>,
    ) -> Result<Response<()>, anemo::rpc::Status> {
        let sender = self
            .sender
            .upgrade()
            .ok_or_else(|| anemo::rpc::Status::internal("shutting down"))?;
        let peer_id = *request
            .peer_id()
            .ok_or_else(|| anemo::rpc::Status::internal("missing peer ID"))?;
        let SendPartialSignaturesRequest { epoch, round, sigs } = request.into_inner();
        let _ = sender // throw away error, caller will retry
            .send(RandomnessMessage::ReceivePartialSignatures(
                peer_id, epoch, round, sigs,
            ))
            .await;
        Ok(anemo::Response::new(()))
    }
}
