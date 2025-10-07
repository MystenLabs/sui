// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! CacheUpdateHandler: broadcast updated objects to local clients over a Unix
//! domain socket when certain transactions are committed (e.g. DEX swaps).
//!
//! Protocol (single message):
//! - 4 bytes: little-endian u32 for payload length
//! - payload: `bcs`-serialized `Vec<(ObjectID, Object)>`

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;
use tokio::io::AsyncWriteExt;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};
use tracing::{error, info, warn};

const SOCKET_PATH: &str = "/tmp/sui/sui_cache_updates.sock";
const ENV_MEV_ENABLED: &str = "ENABLE_SUI_MEV";

#[derive(Debug)]
struct CacheBroadcastMessage {
    objects: Vec<(ObjectID, Object)>,
}

/// A handler for managing connections with external cache update clients.
///
/// When it detects that objects related to specific protocols have been modified,
/// it pushes the updated object data to clients via a Unix socket.
#[derive(Debug)]
pub struct CacheUpdateHandler {
    socket_path: PathBuf,
    connections: Arc<Mutex<Vec<UnixStream>>>,
    // Message queue sender (bounded)
    tx_sender: mpsc::Sender<CacheBroadcastMessage>,
    // Background task handles
    accept_task: Option<JoinHandle<()>>,
    _broadcast_task: Option<JoinHandle<()>>,
    enabled: bool,
    running: Arc<AtomicBool>,
    // Optional bounded send timeout; if None, drop immediately when full.
    send_timeout: Option<Duration>,
}

impl CacheUpdateHandler {
    pub fn new() -> Self {
        let socket_path = PathBuf::from(SOCKET_PATH);

        // Gate by the same env var as TxHandler to keep behavior consistent.
        let enabled = std::env::var(ENV_MEV_ENABLED)
            .ok()
            .map(|v| {
                let v = v.to_ascii_lowercase();
                matches!(v.as_str(), "1" | "true" | "yes" | "on")
            })
            .unwrap_or(false);

        // If enabled, ensure parent dir exists and remove stale/previous socket file.
        if enabled {
            if let Some(parent) = socket_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    warn!(
                        "Failed to create cache socket directory {:?}: {}",
                        parent, e
                    );
                }
            }
            if socket_path.exists() {
                // Try connect: if succeeds, someone is listening; try remove anyway to takeover.
                match std::os::unix::net::UnixStream::connect(&socket_path) {
                    Ok(_) | Err(_) => {
                        if let Err(e) = std::fs::remove_file(&socket_path) {
                            warn!(
                                "Failed to remove pre-existing cache socket {:?}: {}",
                                socket_path, e
                            );
                        }
                    }
                }
            }
        }

        let listener = if enabled {
            let l = UnixListener::bind(&socket_path).unwrap_or_else(|e| {
                panic!(
                    "Failed to bind cache updates Unix socket at {:?}: {}",
                    socket_path, e
                )
            });
            info!("CacheUpdateHandler listening on {:?}", socket_path);
            Some(l)
        } else {
            info!(
                "CacheUpdateHandler disabled: set {}=1 to enable MEV broadcasting",
                ENV_MEV_ENABLED
            );
            None
        };

        let connections = Arc::new(Mutex::new(Vec::new()));

        // Configure bounded queue capacity (default 512), and optional send timeout (default 20ms)
        let cap = std::env::var("SUI_MEV_CACHE_QUEUE_CAP")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(512);
        let send_timeout = std::env::var("SUI_MEV_SEND_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_millis)
            .or_else(|| Some(Duration::from_millis(20)));

        let (tx_sender, tx_receiver) = mpsc::channel::<CacheBroadcastMessage>(cap);

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

    /// Queue message for broadcast
    pub async fn queue_for_broadcast(&self, objects: Vec<(ObjectID, Object)>) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let message = CacheBroadcastMessage { objects };
        if let Some(dur) = self.send_timeout {
            // Wait up to configured timeout; drop on timeout
            match timeout(dur, self.tx_sender.send(message)).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(_)) => Err(anyhow::anyhow!("Cache broadcast task has stopped")),
                Err(_) => {
                    warn!(
                        "Cache broadcast send timed out after {:?}; dropping message",
                        dur
                    );
                    Ok(())
                }
            }
        } else {
            // Non-blocking: drop when full
            match self.tx_sender.try_send(message) {
                Ok(()) => Ok(()),
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    warn!("Cache broadcast queue full; dropping message");
                    Ok(())
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    Err(anyhow::anyhow!("Cache broadcast task has stopped"))
                }
            }
        }
    }

    /// Compatibility wrapper.
    pub async fn notify_written(&self, objects: Vec<(ObjectID, Object)>) {
        let _ = self.queue_for_broadcast(objects).await;
    }

    /// Connection accept loop
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
                    error!("Cache socket accept error: {}", e);
                }
            }
        }
    }

    /// Broadcast task loop
    async fn broadcast_loop(
        mut receiver: mpsc::Receiver<CacheBroadcastMessage>,
        connections: Arc<Mutex<Vec<UnixStream>>>,
    ) {
        while let Some(message) = receiver.recv().await {
            Self::broadcast_once(&message, &connections).await;
        }
    }

    async fn broadcast_once(
        message: &CacheBroadcastMessage,
        connections: &Arc<Mutex<Vec<UnixStream>>>,
    ) {
        // Serialize data
        let serialized = match bcs::to_bytes(&message.objects) {
            Ok(bytes) => bytes,
            Err(e) => {
                error!("Failed to bcs-serialize cache objects: {}", e);
                return;
            }
        };

        let len = serialized.len() as u32;
        let len_bytes = len.to_le_bytes();

        let mut conns = connections.lock().await;
        let mut active_conns = Vec::with_capacity(conns.len());

        // Process connections one by one, remove invalid connections
        while let Some(mut conn) = conns.pop() {
            let res = async {
                conn.write_all(&len_bytes).await?;
                conn.write_all(&serialized).await?;
                Ok::<(), anyhow::Error>(())
            }
            .await;

            if res.is_ok() {
                active_conns.push(conn);
            }
        }

        *conns = active_conns;
    }

    /// Get current connection count
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

    /// Explicit shutdown: stop loops and remove socket file.
    pub fn shutdown(&mut self) {
        if self.enabled {
            self.running.store(false, Ordering::SeqCst);
            if let Some(handle) = self.accept_task.take() {
                handle.abort();
            }
            if let Some(handle) = self._broadcast_task.take() {
                handle.abort();
            }
            if self.socket_path.exists() {
                if let Err(e) = std::fs::remove_file(&self.socket_path) {
                    warn!(
                        "Failed to remove cache socket file {:?} during cleanup: {}",
                        self.socket_path, e
                    );
                }
            }
        }
    }
}

impl Default for CacheUpdateHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CacheUpdateHandler {
    fn drop(&mut self) {
        self.shutdown();
    }
}
