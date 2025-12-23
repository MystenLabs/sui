// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Custom broadcaster for streaming liquidity pool updates via WebSocket.
//!
//! This module provides a WebSocket-based streaming service for DEX/AMM pool state changes,
//! enabling ultra-low-latency access to pool state updates before they are committed to RocksDB.

#[cfg(test)]
use crate::liquidity_decoder::DexProtocol;
use crate::liquidity_decoder::LiquidityPoolState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use base64::Engine;
use futures::stream::StreamExt;
use futures::SinkExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use sui_types::base_types::ObjectID;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::CheckpointTransaction;
use sui_types::object::Object;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, trace, warn};

/// Configuration for the custom broadcaster
#[derive(Debug, Clone)]
pub struct BroadcasterConfig {
    /// Port to listen on for WebSocket connections
    pub port: u16,
    /// Maximum number of concurrent subscribers
    pub max_subscribers: usize,
    /// Channel buffer size for each subscriber
    pub subscriber_buffer_size: usize,
    /// Whether to include raw bytes in updates by default
    pub include_raw_bytes: bool,
}

impl Default for BroadcasterConfig {
    fn default() -> Self {
        Self {
            port: 9003,
            max_subscribers: 1024,
            subscriber_buffer_size: 256,
            include_raw_bytes: false,
        }
    }
}

/// Messages that can be streamed to clients
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamMessage {
    /// A liquidity pool update
    LiquidityUpdate(LiquidityUpdate),
    /// Subscription confirmation
    Subscribed(SubscriptionConfirmation),
    /// Error message
    Error(ErrorMessage),
    /// Heartbeat to keep connection alive
    Heartbeat { timestamp_ms: u64 },
}

/// Liquidity pool update message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityUpdate {
    /// The pool object ID
    pub pool_id: String,
    /// The protocol this pool belongs to
    pub protocol: String,
    /// Full type string of the pool
    pub pool_type: String,
    /// Token types in the pool
    pub token_types: Vec<String>,
    /// Transaction digest that caused this update
    pub digest: String,
    /// Object version after this update
    pub version: u64,
    /// Raw BCS bytes (base64 encoded, optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_bytes: Option<String>,
}

impl From<LiquidityPoolState> for LiquidityUpdate {
    fn from(state: LiquidityPoolState) -> Self {
        Self {
            pool_id: state.pool_id.to_string(),
            protocol: state.protocol.to_string(),
            pool_type: state.pool_type,
            token_types: state.token_types,
            digest: state.digest.to_string(),
            version: state.version,
            raw_bytes: state
                .raw_bytes
                .map(|bytes| base64::engine::general_purpose::STANDARD.encode(&bytes)),
        }
    }
}

/// Subscription confirmation message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionConfirmation {
    /// Subscription ID for this client
    pub subscription_id: String,
    /// Protocols being watched
    pub protocols: Vec<String>,
    /// Specific pool IDs being watched (if any)
    pub pool_ids: Vec<String>,
}

/// Error message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMessage {
    /// Error code
    pub code: String,
    /// Error description
    pub message: String,
}

/// Subscription request from client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Subscribe to liquidity updates
    SubscribeLiquidity(LiquiditySubscription),
    /// Unsubscribe from all updates
    Unsubscribe,
    /// Ping to keep connection alive
    Ping,
}

/// Liquidity subscription parameters
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiquiditySubscription {
    /// Protocol patterns to match (regex). If empty, matches all protocols.
    #[serde(default)]
    pub protocols: Vec<String>,
    /// Specific pool IDs to watch. If empty, watches all pools matching protocols.
    #[serde(default)]
    pub pool_ids: Vec<String>,
    /// Whether to include raw bytes in updates
    #[serde(default)]
    pub include_raw_bytes: bool,
}

/// Active subscription state for a client
#[derive(Debug, Clone)]
struct ActiveSubscription {
    /// Compiled regex patterns for protocols
    protocol_patterns: Vec<Regex>,
    /// Set of specific pool IDs to watch
    pool_ids: HashSet<ObjectID>,
    /// Whether to include raw bytes
    include_raw_bytes: bool,
}

impl ActiveSubscription {
    fn from_request(request: &LiquiditySubscription) -> Result<Self, String> {
        let protocol_patterns = request
            .protocols
            .iter()
            .map(|p| Regex::new(p).map_err(|e| format!("Invalid regex pattern '{}': {}", p, e)))
            .collect::<Result<Vec<_>, _>>()?;

        let pool_ids = request
            .pool_ids
            .iter()
            .map(|id| {
                ObjectID::from_hex_literal(id)
                    .map_err(|e| format!("Invalid pool ID '{}': {}", id, e))
            })
            .collect::<Result<HashSet<_>, _>>()?;

        Ok(Self {
            protocol_patterns,
            pool_ids,
            include_raw_bytes: request.include_raw_bytes,
        })
    }

    fn matches(&self, state: &LiquidityPoolState) -> bool {
        // If specific pool IDs are specified and this pool is in the set, match
        if !self.pool_ids.is_empty() && self.pool_ids.contains(&state.pool_id) {
            return true;
        }

        // If no protocol patterns specified, match all (unless pool IDs were specified)
        if self.protocol_patterns.is_empty() {
            return self.pool_ids.is_empty();
        }

        // Check protocol patterns
        self.protocol_patterns
            .iter()
            .any(|pattern| state.protocol.matches_pattern(pattern))
    }
}

/// Handle to send pool updates to the broadcaster
#[derive(Clone)]
pub struct BroadcasterHandle {
    sender: broadcast::Sender<Arc<LiquidityPoolState>>,
}

impl BroadcasterHandle {
    /// Broadcast a liquidity pool update to all subscribers
    pub fn broadcast(&self, state: LiquidityPoolState) {
        let _ = self.sender.send(Arc::new(state));
    }

    /// Process transaction outputs and broadcast any liquidity pool updates
    pub fn process_transaction(
        &self,
        transaction: &CheckpointTransaction,
        include_raw_bytes: bool,
    ) {
        let digest = *transaction.effects.transaction_digest();

        for object in &transaction.output_objects {
            if let Some(state) =
                LiquidityPoolState::from_object(object, digest, include_raw_bytes)
            {
                trace!(
                    "Broadcasting liquidity update for pool {} ({:?})",
                    state.pool_id,
                    state.protocol
                );
                self.broadcast(state);
            }
        }
    }

    /// Process a batch of objects and broadcast liquidity pool updates
    pub fn process_objects(&self, objects: &[Object], digest: TransactionDigest, include_raw_bytes: bool) {
        for object in objects {
            if let Some(state) = LiquidityPoolState::from_object(object, digest, include_raw_bytes)
            {
                self.broadcast(state);
            }
        }
    }
}

/// Shared state for the broadcaster service
struct BroadcasterState {
    config: BroadcasterConfig,
    update_sender: broadcast::Sender<Arc<LiquidityPoolState>>,
    subscriber_count: RwLock<usize>,
}

/// Custom broadcaster service for WebSocket streaming
pub struct CustomBroadcaster {
    state: Arc<BroadcasterState>,
}

impl CustomBroadcaster {
    /// Create a new broadcaster with the given configuration
    pub fn new(config: BroadcasterConfig) -> (Self, BroadcasterHandle) {
        let (sender, _) = broadcast::channel(config.subscriber_buffer_size * 4);

        let state = Arc::new(BroadcasterState {
            config,
            update_sender: sender.clone(),
            subscriber_count: RwLock::new(0),
        });

        let broadcaster = Self {
            state,
        };

        let handle = BroadcasterHandle { sender };

        (broadcaster, handle)
    }

    /// Start the WebSocket server
    pub async fn start(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.state.config.port));
        info!("Starting custom broadcaster WebSocket server on {}", addr);

        let app = Router::new()
            .route("/ws", get(handle_websocket))
            .route("/health", get(health_check))
            .with_state(self.state);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

/// Health check endpoint
async fn health_check(State(state): State<Arc<BroadcasterState>>) -> impl IntoResponse {
    let count = *state.subscriber_count.read().await;
    format!("OK - {} active subscribers", count)
}

/// WebSocket connection handler
async fn handle_websocket(
    ws: WebSocketUpgrade,
    State(state): State<Arc<BroadcasterState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle an individual WebSocket connection
async fn handle_socket(socket: WebSocket, state: Arc<BroadcasterState>) {
    // Check subscriber limit
    {
        let mut count = state.subscriber_count.write().await;
        if *count >= state.config.max_subscribers {
            warn!("Rejecting connection: max subscribers reached");
            return;
        }
        *count += 1;
    }

    let (mut sender, mut receiver) = socket.split();

    // Create a channel for sending messages to this client
    let (tx, mut rx) = mpsc::channel::<StreamMessage>(state.config.subscriber_buffer_size);

    // Subscribe to broadcast updates
    let mut update_receiver = state.update_sender.subscribe();

    // Client subscription state
    let subscription: Arc<RwLock<Option<ActiveSubscription>>> = Arc::new(RwLock::new(None));
    let subscription_clone = subscription.clone();

    // Spawn task to send messages to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match serde_json::to_string(&msg) {
                Ok(json) => {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to serialize message: {}", e);
                }
            }
        }
    });

    // Spawn task to receive and filter broadcast updates
    let tx_clone = tx.clone();
    let filter_task = tokio::spawn(async move {
        loop {
            match update_receiver.recv().await {
                Ok(pool_state) => {
                    let sub = subscription_clone.read().await;
                    if let Some(active_sub) = sub.as_ref().filter(|s| s.matches(&pool_state)) {
                        let mut update: LiquidityUpdate = (*pool_state).clone().into();
                        // Only include raw bytes if subscription requests it
                        if !active_sub.include_raw_bytes {
                            update.raw_bytes = None;
                        }
                        let msg = StreamMessage::LiquidityUpdate(update);
                        if tx_clone.send(msg).await.is_err() {
                            break;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Subscriber lagged, missed {} updates", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    // Handle incoming messages from client
    let subscription_id = uuid::Uuid::new_v4().to_string();
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::SubscribeLiquidity(sub_request)) => {
                        debug!("Client subscribing to liquidity updates: {:?}", sub_request);
                        match ActiveSubscription::from_request(&sub_request) {
                            Ok(active_sub) => {
                                *subscription.write().await = Some(active_sub);

                                let confirmation = StreamMessage::Subscribed(
                                    SubscriptionConfirmation {
                                        subscription_id: subscription_id.clone(),
                                        protocols: sub_request.protocols.clone(),
                                        pool_ids: sub_request.pool_ids.clone(),
                                    },
                                );
                                let _ = tx.send(confirmation).await;
                            }
                            Err(e) => {
                                let error = StreamMessage::Error(ErrorMessage {
                                    code: "INVALID_SUBSCRIPTION".to_string(),
                                    message: e,
                                });
                                let _ = tx.send(error).await;
                            }
                        }
                    }
                    Ok(ClientMessage::Unsubscribe) => {
                        debug!("Client unsubscribing");
                        *subscription.write().await = None;
                    }
                    Ok(ClientMessage::Ping) => {
                        let heartbeat = StreamMessage::Heartbeat {
                            timestamp_ms: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64,
                        };
                        let _ = tx.send(heartbeat).await;
                    }
                    Err(e) => {
                        let error = StreamMessage::Error(ErrorMessage {
                            code: "PARSE_ERROR".to_string(),
                            message: format!("Failed to parse message: {}", e),
                        });
                        let _ = tx.send(error).await;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                debug!("Client closed connection");
                break;
            }
            Ok(Message::Ping(data)) => {
                // Pong is automatically handled by axum
                trace!("Received ping: {:?}", data);
            }
            Ok(_) => {}
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
        }
    }

    // Cleanup
    send_task.abort();
    filter_task.abort();

    let mut count = state.subscriber_count.write().await;
    *count = count.saturating_sub(1);
    debug!("Client disconnected. Active subscribers: {}", *count);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_matching() {
        let request = LiquiditySubscription {
            protocols: vec!["cetus".to_string()],
            pool_ids: vec![],
            include_raw_bytes: false,
        };

        let sub = ActiveSubscription::from_request(&request).unwrap();

        let state = LiquidityPoolState {
            pool_id: ObjectID::random(),
            protocol: DexProtocol::Cetus,
            pool_type: "test".to_string(),
            token_types: vec![],
            digest: TransactionDigest::random(),
            version: 1,
            raw_bytes: None,
        };

        assert!(sub.matches(&state));

        let state2 = LiquidityPoolState {
            protocol: DexProtocol::Turbos,
            ..state.clone()
        };

        assert!(!sub.matches(&state2));
    }

    #[test]
    fn test_subscription_with_pool_ids() {
        let pool_id = ObjectID::random();
        let request = LiquiditySubscription {
            protocols: vec![],
            pool_ids: vec![pool_id.to_string()],
            include_raw_bytes: false,
        };

        let sub = ActiveSubscription::from_request(&request).unwrap();

        let state = LiquidityPoolState {
            pool_id,
            protocol: DexProtocol::Cetus,
            pool_type: "test".to_string(),
            token_types: vec![],
            digest: TransactionDigest::random(),
            version: 1,
            raw_bytes: None,
        };

        assert!(sub.matches(&state));

        let state2 = LiquidityPoolState {
            pool_id: ObjectID::random(),
            ..state.clone()
        };

        assert!(!sub.matches(&state2));
    }

    #[test]
    fn test_empty_subscription_matches_all() {
        let request = LiquiditySubscription::default();
        let sub = ActiveSubscription::from_request(&request).unwrap();

        let state = LiquidityPoolState {
            pool_id: ObjectID::random(),
            protocol: DexProtocol::Cetus,
            pool_type: "test".to_string(),
            token_types: vec![],
            digest: TransactionDigest::random(),
            version: 1,
            raw_bytes: None,
        };

        assert!(sub.matches(&state));
    }

    #[test]
    fn test_regex_protocol_matching() {
        let request = LiquiditySubscription {
            protocols: vec!["cetus|turbos".to_string()],
            pool_ids: vec![],
            include_raw_bytes: false,
        };

        let sub = ActiveSubscription::from_request(&request).unwrap();

        let cetus_state = LiquidityPoolState {
            pool_id: ObjectID::random(),
            protocol: DexProtocol::Cetus,
            pool_type: "test".to_string(),
            token_types: vec![],
            digest: TransactionDigest::random(),
            version: 1,
            raw_bytes: None,
        };

        let turbos_state = LiquidityPoolState {
            protocol: DexProtocol::Turbos,
            ..cetus_state.clone()
        };

        let deepbook_state = LiquidityPoolState {
            protocol: DexProtocol::DeepBook,
            ..cetus_state.clone()
        };

        assert!(sub.matches(&cetus_state));
        assert!(sub.matches(&turbos_state));
        assert!(!sub.matches(&deepbook_state));
    }

    #[test]
    fn test_client_message_parsing() {
        let json = r#"{"action": "subscribe_liquidity", "protocols": ["cetus"], "pool_ids": [], "include_raw_bytes": false}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::SubscribeLiquidity(_)));

        let json = r#"{"action": "unsubscribe"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::Unsubscribe));

        let json = r#"{"action": "ping"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::Ping));
    }
}
