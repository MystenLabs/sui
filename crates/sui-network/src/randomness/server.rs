// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{Randomness, RandomnessMessage, SendSignaturesRequest};
use anemo::{Request, Response};
use tokio::sync::mpsc;

pub(super) struct Server {
    pub(super) sender: mpsc::WeakSender<RandomnessMessage>,
}

#[anemo::async_trait]
impl Randomness for Server {
    async fn send_signatures(
        &self,
        request: Request<SendSignaturesRequest>,
    ) -> Result<Response<()>, anemo::rpc::Status> {
        let sender = self
            .sender
            .upgrade()
            .ok_or_else(|| anemo::rpc::Status::internal("shutting down"))?;
        let peer_id = *request
            .peer_id()
            .ok_or_else(|| anemo::rpc::Status::internal("missing peer ID"))?;
        let SendSignaturesRequest {
            epoch,
            round,
            partial_sigs,
            sig,
        } = request.into_inner();
        let _ = sender // throw away error, caller will retry
            .send(RandomnessMessage::ReceiveSignatures(
                peer_id,
                epoch,
                round,
                partial_sigs,
                sig,
            ))
            .await;
        Ok(anemo::Response::new(()))
    }
}
