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
    transaction::{TransactionDataAPI, TransactionKind}, 
    effects::TransactionEffectsAPI,
};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info};

// --- Data Structures ---

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SubscriptionRequest {
    SubscribePool(ObjectID),
    SubscribeAccount(SuiAddress),
    SubscribeOrders(SuiAddress), 
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
        is_success: bool,
        commands: Option<Vec<u8>>,
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
    OrderPlaced {
        order_id: String,
        sender: SuiAddress,
        digest: String,
        is_success: bool,
    },
    ProbeEvent {
        event_type: String,
        sender: SuiAddress,
        contents: Vec<u8>,
        digest: String,
    },
    SubscriptionSuccess {
        details: String,
    },
    SubscriptionError {
        reason: String,
        input: String,
        hint: String,
    },
    Raw(SerializableOutput),
}

#[derive(Clone, Debug, Serialize)]
pub struct SerializableOutput {
    digest: String,
    timestamp_ms: u64,
}

struct AppState {
    tx: broadcast::Sender<Arc<TransactionOutputs>>,
}

pub struct CustomBroadcaster;

impl CustomBroadcaster {
    pub fn spawn(mut rx: mpsc::Receiver<Arc<TransactionOutputs>>, port: u16) {
        let (tx, _) = broadcast::channel(1000);
        let tx_clone = tx.clone();

        tokio::spawn(async move {
            info!("CustomBroadcaster: Ingestion loop started");
            while let Some(outputs) = rx.recv().await {
                if let Err(_) = tx_clone.send(outputs) {
                    debug!("CustomBroadcaster: No active subscribers");
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

            if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
                let _ = axum::serve(listener, app.into_make_service()).await;
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
    let mut subscriptions_orders = HashSet::new();
    let mut subscribe_all = false;

    println!("[DEBUG] 新的 WebSocket 客戶端已連入！");

    loop {
        tokio::select! {
            res = rx.recv() => {
                match res {
                    Ok(outputs) => {
                        let digest = outputs.transaction.digest().to_string();
                        let sender = outputs.transaction.sender_address();
                        let is_success = outputs.effects.status().is_ok();
                        let tx_data = outputs.transaction.transaction_data(); 
                        let tx_kind = tx_data.kind();
                        
                        let commands_bytes = match tx_kind {
                            TransactionKind::ProgrammableTransaction(pt) => {
                                bcs::to_bytes(&pt.commands).ok()
                            },
                            _ => None
                        };

                        if subscribe_all {
                             let msg = StreamMessage::AccountActivity {
                                 account: sender,
                                 digest: digest.clone(),
                                 kind: "Transaction".to_string(),
                                 is_success,
                                 commands: commands_bytes.clone(),
                             };
                             if let Err(_) = send_json(&mut socket, &msg).await { break; }
                        }

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

                             if subscriptions_orders.contains(&event.sender) {
                                 let probe = StreamMessage::ProbeEvent {
                                     event_type: event.type_.to_string(),
                                     sender: event.sender,
                                     contents: event.contents.clone(),
                                     digest: digest.clone(),
                                 };
                                 let _ = send_json(&mut socket, &probe).await;

                                 if event.type_.to_string().contains("OrderPlaced") {
                                     let msg = StreamMessage::OrderPlaced {
                                         order_id: "PENDING_DECODE".to_string(),
                                         sender: event.sender,
                                         digest: digest.clone(),
                                         is_success,
                                     };
                                     let _ = send_json(&mut socket, &msg).await;
                                 }
                             }
                        }

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

                        if subscriptions_accounts.contains(&sender) {
                             let msg = StreamMessage::AccountActivity {
                                 account: sender,
                                 digest: digest.clone(),
                                 kind: "Transaction".to_string(),
                                 is_success,
                                 commands: commands_bytes,
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
                            match serde_json::from_str::<SubscriptionRequest>(&text) {
                                Ok(req) => {
                                    println!("✅ [DEBUG] 收到訂閱: {:?}", req);
                                    let _ = send_json(&mut socket, &StreamMessage::SubscriptionSuccess {
                                        details: format!("成功訂閱 {:?}", req),
                                    }).await;

                                    match req {
                                        SubscriptionRequest::SubscribePool(id) => { subscriptions_pools.insert(id); }
                                        SubscriptionRequest::SubscribeAccount(addr) => { subscriptions_accounts.insert(addr); }
                                        SubscriptionRequest::SubscribeOrders(addr) => { subscriptions_orders.insert(addr); }
                                        SubscriptionRequest::SubscribeAll => { subscribe_all = true; }
                                    }
                                }
                                Err(e) => {
                                    let _ = send_json(&mut socket, &StreamMessage::SubscriptionError {
                                        reason: e.to_string(),
                                        // 修復 E0308: 明確轉換為 String
                                        input: text.to_string(), 
                                        hint: "如果是 SubscribeAll，請直接輸入 \"SubscribeAll\"".to_string(),
                                    }).await;
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
        println!("🚀 探針版已啟動於 9003 埠口...");
        loop { tokio::time::sleep(Duration::from_secs(10)).await; }
    }
}