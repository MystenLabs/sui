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
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    transaction::TransactionDataAPI,
};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

// --- Data Structures ---

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SubscriptionRequest {
    SubscribePool(ObjectID),
    SubscribeAccount(SuiAddress),
    SubscribeOrders(SuiAddress), // [Ticket #2] 新增訂單訂閱
    SubscribeAll,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum StreamMessage {
    PoolUpdate {
        pool_id: ObjectID,
        digest: String,
        object: Option<Vec<u8>>,
    },
    AccountActivity {
        account: SuiAddress,
        digest: String,
        kind: String,
    },
    BalanceChange {
        account: SuiAddress,
        coin_type: String,
        new_balance: u64,
    },
    Event {
        package_id: ObjectID,
        transaction_module: String,
        sender: SuiAddress,
        type_: String,
        contents: Vec<u8>,
        digest: String,
    },
    // [Ticket #2] 新增訂單相關訊息與探針
    OrderPlaced {
        order_id: String,
        sender: SuiAddress,
        digest: String,
    },
    ProbeEvent {
        event_type: String,
        sender: SuiAddress,
        contents_hex: String,
    },
    SubscriptionSuccess {
        details: String,
    },
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
        let (tx, _) = broadcast::channel(1000);
        let tx_clone = tx.clone();

        tokio::spawn(async move {
            info!("CustomBroadcaster: Ingestion loop started");
            while let Some(outputs) = rx.recv().await {
                if let Err(e) = tx_clone.send(outputs) {
                    debug!("CustomBroadcaster: No active subscribers: {}", e);
                }
            }
        });

        let app_state = Arc::new(AppState { tx });
        tokio::spawn(async move {
            let app = Router::new()
                .route("/ws", get(ws_handler))
                .with_state(app_state);

            let addr = SocketAddr::from(([0, 0, 0, 0], port));
            info!("CustomBroadcaster: Listening on {}", addr);

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

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.tx.subscribe();
    let mut subscriptions_pools = HashSet::new();
    let mut subscriptions_accounts = HashSet::new();
    let mut subscriptions_orders = HashSet::new(); // [Ticket #2]
    let mut subscribe_all = false;

    println!("📡 [DEBUG] 新的 WebSocket 客戶端已連入！");

    loop {
        tokio::select! {
            res = rx.recv() => {
                match res {
                    Ok(outputs) => {
                        let digest = outputs.transaction.digest().to_string();
                        let sender = outputs.transaction.sender_address();

                        // 1. Firehose / SubscribeAll
                        if subscribe_all {
                             let msg = StreamMessage::AccountActivity {
                                 account: sender,
                                 digest: digest.clone(),
                                 kind: "Transaction".to_string(),
                             };
                             let _ = send_json(&mut socket, &msg).await;
                        }

                        // 2. Events Broadcast & [Ticket #2] Order Detection
                        for event in &outputs.events.data {
                             if subscribe_all {
                                 let msg = StreamMessage::Event {
                                     package_id: event.package_id,
                                     transaction_module: event.transaction_module.to_string(),
                                     sender: event.sender,
                                     type_: event.type_.to_string(),
                                     contents: event.contents.clone(),
                                     digest: digest.clone(),
                                 };
                                 let _ = send_json(&mut socket, &msg).await;
                             }

                             // --- [Ticket #2] 訂單探針邏輯 ---
                             if subscriptions_orders.contains(&event.sender) {
                                 let hex_contents = format!("{:02x?}", event.contents);
                                 println!("🔍 [探針] 發現目標 {} 的事件: {}", event.sender, event.type_);
                                 
                                 let probe = StreamMessage::ProbeEvent {
                                     event_type: event.type_.to_string(),
                                     sender: event.sender,
                                     contents_hex: hex_contents,
                                 };
                                 let _ = send_json(&mut socket, &probe).await;

                                 if event.type_.to_string().contains("OrderPlaced") {
                                     let msg = StreamMessage::OrderPlaced {
                                         order_id: "PENDING".to_string(),
                                         sender: event.sender,
                                         digest: digest.clone(),
                                     };
                                     let _ = send_json(&mut socket, &msg).await;
                                 }
                             }
                        }

                        // 3. Pool Updates
                        for (id, object) in &outputs.written {
                             if subscriptions_pools.contains(id) {
                                  let object_bytes = object.data.try_as_move().map(|o| o.contents().to_vec());
                                  let msg = StreamMessage::PoolUpdate {
                                      pool_id: *id,
                                      digest: digest.clone(),
                                      object: object_bytes,
                                  };
                                  let _ = send_json(&mut socket, &msg).await;
                             }
                        }

                        // 4. Account Updates
                        if subscriptions_accounts.contains(&sender) {
                             let msg = StreamMessage::AccountActivity {
                                 account: sender,
                                 digest: digest.clone(),
                                 kind: "Transaction".to_string(),
                             };
                             let _ = send_json(&mut socket, &msg).await;
                        }
                    }
                    Err(_) => break,
                }
            }

            res = socket.recv() => {
                match res {
                    Some(Ok(msg)) => {
                        if let Message::Text(text) = msg {
                            if let Ok(req) = serde_json::from_str::<SubscriptionRequest>(&text) {
                                println!("✅ [DEBUG] 收到訂閱: {:?}", req);
                                let ack = StreamMessage::SubscriptionSuccess {
                                    details: format!("成功訂閱 {:?}", req),
                                };
                                let _ = send_json(&mut socket, &ack).await;

                                match req {
                                    SubscriptionRequest::SubscribePool(id) => { subscriptions_pools.insert(id); }
                                    SubscriptionRequest::SubscribeAccount(addr) => { subscriptions_accounts.insert(addr); }
                                    SubscriptionRequest::SubscribeOrders(addr) => { subscriptions_orders.insert(addr); }
                                    SubscriptionRequest::SubscribeAll => { subscribe_all = true; }
                                }
                            }
                        } else if let Message::Close(_) = msg {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        }
    }
}

async fn send_json<T: Serialize>(socket: &mut WebSocket, msg: &T) -> Result<(), ()> {
    let text = serde_json::to_string(msg).map_err(|_| ())?;
    socket.send(Message::Text(text.into())).await.map_err(|_| ())
}

#[cfg(test)]
mod smoke_tests {
    use super::*;
    use std::time::Duration;
    #[tokio::test]
    async fn test_broadcaster_startup() {
        let (_tx, rx) = mpsc::channel(100);
        CustomBroadcaster::spawn(rx, 9003);
        println!("🚀 探針測試版已啟動於 9003...");
        loop { tokio::time::sleep(Duration::from_secs(10)).await; }
    }
}