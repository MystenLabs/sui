// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use sui_types::event::EventEnvelope;
use tokio::{sync::mpsc::Receiver, task::JoinHandle};
use tracing::debug;

pub struct Streamer {
    event_queue: Receiver<EventEnvelope>,
}

impl Streamer {
    pub fn spawn(rx: Receiver<EventEnvelope>) -> JoinHandle<()> {
        tokio::spawn(async move { Self { event_queue: rx }.stream().await })
    }

    pub async fn stream(&mut self) {
        while let Some(event_envelope) = self.event_queue.recv().await {
            debug!(event_envelope =? event_envelope, "Get event");
        }
    }
}
