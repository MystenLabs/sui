// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::sync::Arc;
use sui_types::error::SuiError;
use tokio::sync::mpsc::Sender;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::BroadcastStream;
use tracing::{debug, warn};

pub struct Streamer<T> {
    streamer_queue: Sender<T>,
    subscribers: Arc<broadcast::Sender<T>>,
}

impl<T> Streamer<T>
where
    T: Clone + Debug + Send + 'static,
{
    pub fn spawn(buffer: usize) -> Self {
        let (tx, rx) = mpsc::channel::<T>(buffer);
        let streamer = Self {
            streamer_queue: tx,
            subscribers: Arc::new(broadcast::channel(16).0),
        };
        let mut rx = rx;
        let subscribers = streamer.subscribers.clone();

        tokio::spawn(async move {
            while let Some(data) = rx.recv().await {
                debug!(data =? data, "Get event");
                if subscribers.receiver_count() > 0 {
                    match subscribers.send(data) {
                        Ok(num) => {
                            debug!("Broadcast Move event to {num} peers.")
                        }
                        Err(e) => {
                            warn!("Error broadcasting event. Error: {e}")
                        }
                    }
                }
            }
        });
        streamer
    }

    pub fn subscribe(&self) -> BroadcastStream<T> {
        BroadcastStream::new(self.subscribers.subscribe())
    }

    pub async fn send(&self, data: T) -> Result<(), SuiError> {
        self.streamer_queue
            .send(data)
            .await
            .map_err(|e| SuiError::EventFailedToDispatch {
                error: e.to_string(),
            })
    }
}
