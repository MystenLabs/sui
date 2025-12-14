use crate::transaction_outputs::TransactionOutputs;
use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, net::SocketAddr, sync::Arc};
#[allow(unused_imports)]
use sui_types::transaction::TransactionDataAPI;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    transaction::TransactionDataAPI, // Kept if needed for trait bounds, but suppressing warning if unused
};
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};

// --- Data Structures ---

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SubscriptionRequest {
    SubscribePool(ObjectID),
    SubscribeAccount(SuiAddress),
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum StreamMessage {
    PoolUpdate {
        pool_id: ObjectID,
        digest: String,
        // Add more specific fields here, e.g., square root price if available immediately
        // For now we stream the event or the object change notification
    },
    AccountActivity {
        account: SuiAddress,
        digest: String,
        kind: String, // e.g., "Swap", "Transfer"
    },
    BalanceChange {
        account: SuiAddress,
        coin_type: String,
        new_balance: u64,
    },
    // Raw output for advanced filtering
    Raw(SerializableOutput),
}

#[derive(Clone, Debug, Serialize)]
pub struct SerializableOutput {
    digest: String,
    timestamp_ms: u64,
}

// --- Broadcaster State ---

struct AppState {
    tx: broadcast::Sender<Arc<TransactionOutputs>>,
}

// --- Main Broadcaster Logic ---

pub struct CustomBroadcaster;

impl CustomBroadcaster {
    pub fn spawn(mut rx: mpsc::Receiver<Arc<TransactionOutputs>>, port: u16) {
        // Create a broadcast channel for all connected websocket clients
        // Capacity 1000 to handle bursts
        let (tx, _) = broadcast::channel(1000);
        let tx_clone = tx.clone();

        // 1. Spawn the ingestion loop
        tokio::spawn(async move {
            info!("CustomBroadcaster: Ingestion loop started");
            while let Some(outputs) = rx.recv().await {
                // Determine if this output is "interesting" before broadcasting?
                // Or broadcast everything and let per-client filters handle it?
                // For low latency, we broadcast raw or minimally processed data.

                // We broadcast the Arc directly to avoid cloning the heavy data structure.
                // The serialization happens in the client handling task.
                if let Err(e) = tx_clone.send(outputs) {
                    warn!(
                        "CustomBroadcaster: No active subscribers, dropped message: {}",
                        e
                    );
                }
            }
            info!("CustomBroadcaster: Ingestion loop ended");
        });

        // 2. Spawn the WebServer
        let app_state = Arc::new(AppState { tx });

        tokio::spawn(async move {
            let app = Router::new()
                .route("/ws", get(ws_handler))
                .with_state(app_state);

            let addr = SocketAddr::from(([0, 0, 0, 0], port));
            info!("CustomBroadcaster: Listening on {}", addr);

            // Fix for new Axum version: use tokio::net::TcpListener
            match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => {
                    if let Err(e) = axum::serve(listener, app.into_make_service()).await {
                        error!("CustomBroadcaster: Server error: {}", e);
                    }
                }
                Err(e) => {
                    error!("CustomBroadcaster: Failed to bind to address: {}", e);
                }
            }
        });
    }
}

// --- WebSocket Handling ---

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.tx.subscribe();

    // In a real implementation, we would read the first message as a subscription request.
    // For now, we assume interest in EVERYTHING or let them filter client side?
    // Requirement says: "filter only interesting updates (e.g. pool objects, coin objects)"
    // So let's accept a JSON subscription message.

    let mut subscriptions_pools = HashSet::new();
    let mut subscriptions_accounts = HashSet::new();

    // Wait for subscription message (simple handshake)
    if let Some(Ok(msg)) = socket.recv().await {
        if let Message::Text(text) = msg {
            // In newer Axum/Tungstenite, Text contains Utf8Bytes which works like a string.
            // We access it as a str for parsing.
            if let Ok(req) = serde_json::from_str::<SubscriptionRequest>(&text) {
                info!("Client subscribed: {:?}", req);
                match req {
                    SubscriptionRequest::SubscribePool(id) => {
                        subscriptions_pools.insert(id);
                    }
                    SubscriptionRequest::SubscribeAccount(addr) => {
                        subscriptions_accounts.insert(addr);
                    }
                }
            }
        }
    }

    loop {
        tokio::select! {
            // Outbound: Send updates to client
            Ok(outputs) = rx.recv() => {
                let digest = outputs.transaction.digest();

                // Helper to check if transaction involves a pool
                let mut match_found = false;

                // Check Written objects for Pool IDs
                // Fix: loop over keys() returns &ObjectID directly, no tuple pattern matching needed
                for id in outputs.written.keys() {
                    if subscriptions_pools.contains(id) {
                         let msg = StreamMessage::PoolUpdate {
                             pool_id: *id,
                             digest: digest.to_string(),
                         };
                         let _ = send_json(&mut socket, &msg).await;
                         match_found = true;
                         break;
                    }
                }

                if match_found { continue; }

                // Check Transaction Sender for Account Activity
                let sender = outputs.transaction.sender_address();
                if subscriptions_accounts.contains(&sender) {
                     let msg = StreamMessage::AccountActivity {
                         account: sender,
                         digest: digest.to_string(),
                         kind: "Transaction".to_string(),
                     };
                     let _ = send_json(&mut socket, &msg).await;
                     match_found = true;
                }

                if match_found { continue; }

                // Check mutated objects for Account ownership (simplified for balance)
                 for _id in outputs.written.keys() {
                    // This is hard without full object parsing, but we can verify ownership in full implementation
                    // For now, if we don't have the object data readily available as specific types, we assume client wants raw?
                 }
            }

            // Inbound: Handle control messages or disconnects
            else => break,
        }
    }
}

async fn send_json<T: Serialize>(socket: &mut WebSocket, msg: &T) -> Result<(), ()> {
    let text = serde_json::to_string(msg).map_err(|_| ())?;
    // Fix: Convert String to Utf8Bytes via .into()
    socket
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| ())
}
