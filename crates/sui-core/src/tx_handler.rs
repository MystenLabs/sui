// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! TxHandler: broadcast committed transaction effects and events to local clients
//! over a Unix domain socket. This is intended for local analytics (e.g. MEV).
//!
//! Protocol (single message):
//! - 4 bytes: big-endian u32 for effects payload length
//! - effects payload: `bincode`-serialized `TransactionEffects`
//! - 4 bytes: big-endian u32 for events payload length
//! - events payload: JSON array of `sui_json_rpc_types::SuiEvent`

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use sui_json_rpc_types::SuiEvent;
use sui_types::effects::TransactionEffects;
use tokio::io::AsyncWriteExt;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};
use tracing::{error, info, warn};

pub const TX_SOCKET_PATH: &str = "/tmp/sui/sui_tx.sock";
const ENV_MEV_ENABLED: &str = "ENABLE_SUI_MEV";

#[derive(Debug)]
struct BroadcastMessage {
    effects: TransactionEffects,
    events: Vec<SuiEvent>,
}

/// A handler for managing connections with external tx clients.
///
/// It accepts connections via a Unix socket and asynchronously broadcasts
/// committed `TransactionEffects` plus `SuiEvent`s to all active clients.
#[derive(Debug)]
pub struct TxHandler {
    socket_path: PathBuf,
    connections: Arc<Mutex<Vec<UnixStream>>>,
    // Message queue sender (bounded)
    tx_sender: mpsc::Sender<BroadcastMessage>,
    // Background task handle
    accept_task: Option<JoinHandle<()>>,
    _broadcast_task: Option<JoinHandle<()>>,
    // Whether MEV broadcasting is enabled (gated by env var).
    enabled: bool,
    running: Arc<AtomicBool>,
    // Optional bounded send timeout; if None, drop immediately when full.
    send_timeout: Option<Duration>,
}

impl TxHandler {
    pub fn new() -> Self {
        let socket_path = PathBuf::from(TX_SOCKET_PATH);

        // Gate by env var. Accept common truthy values.
        let enabled = std::env::var(ENV_MEV_ENABLED)
            .ok()
            .map(|v| {
                let v = v.to_ascii_lowercase();
                matches!(v.as_str(), "1" | "true" | "yes" | "on")
            })
            .unwrap_or(false);

        // Ensure parent dir exists to avoid bind errors.
        if enabled {
            if let Some(parent) = socket_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    warn!("Failed to create tx socket directory {:?}: {}", parent, e);
                }
            }
            // Remove stale socket file if no one is listening.
            if socket_path.exists() {
                match std::os::unix::net::UnixStream::connect(&socket_path) {
                    Ok(_) => {
                        // Someone is already listening; attempt to remove to takeover.
                        if let Err(e) = std::fs::remove_file(&socket_path) {
                            warn!(
                                "Failed to remove pre-existing tx socket {:?}: {}",
                                socket_path, e
                            );
                        }
                    }
                    Err(_) => {
                        // Stale file; safe to remove.
                        if let Err(e) = std::fs::remove_file(&socket_path) {
                            warn!("Failed to remove stale tx socket {:?}: {}", socket_path, e);
                        }
                    }
                }
            }
        }

        let listener = if enabled {
            let l = UnixListener::bind(&socket_path).unwrap_or_else(|e| {
                panic!("Failed to bind tx Unix socket at {:?}: {}", socket_path, e)
            });
            info!("TxHandler listening on {:?}", socket_path);
            Some(l)
        } else {
            info!(
                "TxHandler disabled: set {}=1 to enable MEV broadcasting",
                ENV_MEV_ENABLED
            );
            None
        };

        let connections = Arc::new(Mutex::new(Vec::new()));

        // Configure bounded queue capacity (default 2048), and optional send timeout (default 20ms)
        let cap = std::env::var("SUI_MEV_TX_QUEUE_CAP")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(2048);
        let send_timeout = std::env::var("SUI_MEV_SEND_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_millis)
            .or_else(|| Some(Duration::from_millis(20)));

        // Create bounded message queue
        let (tx_sender, tx_receiver) = mpsc::channel::<BroadcastMessage>(cap);

        // Start tasks only if enabled.
        let running = Arc::new(AtomicBool::new(enabled));
        let accept_task = if let Some(listener) = listener {
            let connections_for_accept = connections.clone();
            let running_for_accept = running.clone();
            Some(tokio::spawn(async move {
                Self::accept_loop(listener, connections_for_accept, running_for_accept).await;
            }))
        } else {
            None
        };

        let broadcast_task = if enabled {
            let connections_for_broadcast = connections.clone();
            Some(tokio::spawn(async move {
                Self::broadcast_loop(tx_receiver, connections_for_broadcast).await;
            }))
        } else {
            // Drop receiver; queue ops will be short-circuited.
            drop(tx_receiver);
            None
        };

        Self {
            socket_path,
            connections,
            tx_sender,
            accept_task,
            _broadcast_task: broadcast_task,
            enabled,
            running,
            send_timeout,
        }
    }

    /// Queue a message to broadcast to all active clients.
    pub async fn queue_for_broadcast(
        &self,
        effects: TransactionEffects,
        events: Vec<SuiEvent>,
    ) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let msg = BroadcastMessage { effects, events };
        if let Some(dur) = self.send_timeout {
            // Wait up to configured timeout; drop on timeout
            match timeout(dur, self.tx_sender.send(msg)).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(_)) => Err(anyhow::anyhow!("Tx broadcast task stopped")),
                Err(_) => {
                    warn!(
                        "Tx broadcast send timed out after {:?}; dropping message",
                        dur
                    );
                    Ok(())
                }
            }
        } else {
            // Non-blocking: drop when full
            match self.tx_sender.try_send(msg) {
                Ok(()) => Ok(()),
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    warn!("Tx broadcast queue full; dropping message");
                    Ok(())
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    Err(anyhow::anyhow!("Tx broadcast task stopped"))
                }
            }
        }
    }

    /// Compatibility wrapper retained from the reference design.
    pub async fn send_tx_effects_and_events(
        &self,
        effects: &TransactionEffects,
        events: Vec<SuiEvent>,
    ) -> Result<()> {
        self.queue_for_broadcast(effects.clone(), events).await
    }

    async fn accept_loop(
        listener: UnixListener,
        connections: Arc<Mutex<Vec<UnixStream>>>,
        running: Arc<AtomicBool>,
    ) {
        while running.load(Ordering::SeqCst) {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let mut conns = connections.lock().await;
                    conns.push(stream);
                }
                Err(e) => {
                    error!("Tx socket accept error: {}", e);
                }
            }
        }
    }

    async fn broadcast_loop(
        mut receiver: mpsc::Receiver<BroadcastMessage>,
        connections: Arc<Mutex<Vec<UnixStream>>>,
    ) {
        while let Some(msg) = receiver.recv().await {
            Self::broadcast_once(&msg, &connections).await;
        }
    }

    async fn broadcast_once(msg: &BroadcastMessage, connections: &Arc<Mutex<Vec<UnixStream>>>) {
        // Serialize effects and events.
        let effects_bytes = match bincode::serialize(&msg.effects) {
            Ok(b) => b,
            Err(e) => {
                error!("Failed to serialize effects: {}", e);
                return;
            }
        };
        let events_bytes = match serde_json::to_vec(&msg.events) {
            Ok(b) => b,
            Err(e) => {
                error!("Failed to serialize events to JSON: {}", e);
                return;
            }
        };

        let effects_len = (effects_bytes.len() as u32).to_be_bytes();
        let events_len = (events_bytes.len() as u32).to_be_bytes();

        // Drain inactive connections.
        let mut conns = connections.lock().await;
        let mut active = Vec::with_capacity(conns.len());

        while let Some(mut conn) = conns.pop() {
            let res = async {
                conn.write_all(&effects_len).await?;
                conn.write_all(&effects_bytes).await?;
                conn.write_all(&events_len).await?;
                conn.write_all(&events_bytes).await?;
                Ok::<(), anyhow::Error>(())
            }
            .await;

            if res.is_ok() {
                active.push(conn);
            }
        }

        *conns = active;
    }

    /// Get the current number of connected clients.
    pub fn connection_count(&self) -> usize {
        if !self.enabled {
            0
        } else {
            self.connections.try_lock().map(|c| c.len()).unwrap_or(0)
        }
    }

    /// Whether MEV broadcasting is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for TxHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TxHandler {
    fn drop(&mut self) {
        // Attempt a graceful shutdown.
        self.shutdown();
    }
}

impl TxHandler {
    /// Explicitly shutdown the handler: stop loops, abort tasks, remove socket file.
    pub fn shutdown(&mut self) {
        if self.enabled {
            self.running.store(false, Ordering::SeqCst);
            if let Some(handle) = self.accept_task.take() {
                handle.abort();
            }
            if let Some(handle) = self._broadcast_task.take() {
                handle.abort();
            }
            // Best effort cleanup of socket file.
            if self.socket_path.exists() {
                if let Err(e) = std::fs::remove_file(&self.socket_path) {
                    warn!(
                        "Failed to remove tx socket file {:?} during cleanup: {}",
                        self.socket_path, e
                    );
                }
            }
        }
    }
}
