// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use sui_types::event::EventEnvelope;
use tokio::sync::broadcast;
use tokio::sync::mpsc::Receiver;
use tokio_stream::wrappers::BroadcastStream;
use tracing::{debug, warn};

pub struct Streamer {
    subscribers: Arc<broadcast::Sender<EventEnvelope>>,
}

impl Streamer {
    pub fn spawn(rx: Receiver<EventEnvelope>) -> Self {
        let streamer = Self {
            subscribers: Arc::new(broadcast::channel(16).0),
        };
        let mut rx = rx;
        let subscribers = streamer.subscribers.clone();
        tokio::spawn(async move {
            while let Some(event_envelope) = rx.recv().await {
                debug!(event_envelope =? event_envelope, "Get event");
                match subscribers.send(event_envelope) {
                    Ok(num) => {
                        debug!("Broadcast Move event to {num} peers.")
                    }
                    Err(e) => {
                        warn!("Error broadcasting event. Error: {e}")
                    }
                }
            }
        });
        streamer
    }

    pub fn subscribe(&self) -> BroadcastStream<EventEnvelope> {
        BroadcastStream::new(self.subscribers.subscribe())
    }
}
