use std::collections::BTreeMap;
use std::sync::Arc;

use jsonrpsee_core::error::SubscriptionClosed;
use jsonrpsee_core::server::rpc_module::PendingSubscription;
use jsonrpsee_proc_macros::rpc;
use serde::Deserialize;
use serde::Serialize;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use tokio::sync::broadcast;
use tokio::sync::broadcast::Sender;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tracing::{debug, warn};

use sui_core::gateway_types::SuiEvent;

#[rpc(server, client, namespace = "sui")]
pub trait EventApi {
    #[subscription(name = "subEvent", unsubscribe = "unsubEvent", item = SuiEvent)]
    fn sub_event(&self, key: EventType);
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize, Copy, Clone, Debug, EnumIter)]
pub enum EventType {
    Foo,
    Bar,
}

pub struct SuiEventManager {
    txs: BTreeMap<EventType, Sender<SuiEvent>>,
}

impl Default for SuiEventManager {
    fn default() -> Self {
        Self {
            txs: EventType::iter()
                .map(|ty| (ty, broadcast::channel(16).0))
                .collect(),
        }
    }
}

impl SuiEventManager {
    pub fn broadcast(&self, key: EventType, event: SuiEvent) {
        let tx = &self.txs[&key];
        if tx.receiver_count() > 0 {
            match tx.send(event) {
                Ok(num) => {
                    debug!("Broadcast [{key:?}] event to {num} peers.")
                }
                Err(e) => {
                    warn!("Error broadcasting [{key:?}] event. Error: {e}")
                }
            }
        }
    }

    pub fn broadcast_stream(&self, key: EventType) -> BroadcastStream<SuiEvent> {
        let tx = &self.txs[&key];
        BroadcastStream::new(tx.clone().subscribe())
    }
}

pub struct EventApiImpl {
    manager: Arc<SuiEventManager>,
}

impl EventApiImpl {
    pub fn new(manager: Arc<SuiEventManager>) -> Self {
        Self { manager }
    }
}

impl EventApiServer for EventApiImpl {
    fn sub_event(&self, pending: PendingSubscription, key: EventType) {
        let rx = self.manager.broadcast_stream(key);
        let mut sink = match pending.accept() {
            Some(sink) => sink,
            _ => return,
        };
        // TODO: We can apply additional filter to the broadcast stream
        tokio::spawn(async move {
            match sink.pipe_from_try_stream(rx).await {
                SubscriptionClosed::Success => {
                    sink.close(SubscriptionClosed::Success);
                }
                SubscriptionClosed::RemotePeerAborted => (),
                SubscriptionClosed::Failed(err) => {
                    sink.close(err);
                }
            };
        });
    }
}
