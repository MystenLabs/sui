// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! TCP proxy for testing graphql streaming subscription's gap-recovery behavior.
//!
//! # Why this exists
//!
//! Graphql streaming subscriptions reconnect on upstream gRPC stream errors and
//! recover any gap from kv-rpc (see
//! `sui-indexer-alt-graphql/src/task/streaming/checkpoint_stream_task.rs`). To
//! e2e test that recovery, we need a way to trigger an upstream disconnect
//! mid-test, but the validator's gRPC just keeps running and graphql's
//! connection stays healthy on its own.
//!
//! # What it does
//!
//! The proxy sits between graphql and the upstream:
//!
//! ```text
//!   graphql ──TCP──▶ proxy ──TCP──▶ upstream (validator gRPC)
//!                      │
//!                      ▼ disconnect_all()
//! ```
//!
//! Tests call `disconnect_all()` to abort every active forwarding task,
//! closing both ends of each connection. graphql sees a TCP close, tonic
//! surfaces a stream error, and the streaming task's reconnect + gap-recovery
//! code path runs.
//!
//! To force a *deterministic* gap (rather than relying on timing luck for the
//! validator to advance during the reconnect window), tests can additionally
//! call `block_connections()` before `disconnect_all()` and `allow_connections()`
//! after a known sleep. While blocked, the proxy accepts each inbound TCP
//! connection and immediately drops it, so graphql's reconnect attempts fail
//! for the full blackout window. The validator keeps producing checkpoints
//! during that window, so when reconnect finally succeeds, gap recovery has a
//! known-non-empty range to fill.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::task::AbortHandle;

/// Controls the proxy's active connections. Cheap to clone.
#[derive(Clone, Default)]
pub struct ProxyController {
    active: Arc<Mutex<Vec<AbortHandle>>>,
    blocked: Arc<AtomicBool>,
}

impl ProxyController {
    /// Abort every active forwarding task. Connections made after this call
    /// still work normally unless `block_connections()` was called first.
    pub fn disconnect_all(&self) {
        for handle in self.active.lock().unwrap().drain(..) {
            handle.abort();
        }
    }

    /// Reject new inbound connections until `allow_connections()` is called.
    /// Combined with `disconnect_all()`, this lets a test guarantee a reconnect
    /// blackout of a known minimum duration, forcing the validator to advance
    /// past the disconnect point so a real gap forms.
    pub fn block_connections(&self) {
        self.blocked.store(true, Ordering::Release);
    }

    /// Resume accepting inbound connections.
    pub fn allow_connections(&self) {
        self.blocked.store(false, Ordering::Release);
    }
}

/// Start a TCP proxy forwarding to `upstream_url`. Returns the proxy's URL
/// (which graphql should connect to instead of the upstream) and a controller
/// for triggering disconnects.
pub async fn start(upstream_url: &str) -> (String, ProxyController) {
    let upstream_addr = upstream_url
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .to_string();

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind proxy");
    let local_port = listener
        .local_addr()
        .expect("Failed to get local addr")
        .port();

    let controller = ProxyController::default();
    let active = controller.active.clone();
    let blocked = controller.blocked.clone();

    tokio::spawn(async move {
        loop {
            let (inbound, _) = match listener.accept().await {
                Ok(io) => io,
                Err(_) => return,
            };
            if blocked.load(Ordering::Acquire) {
                // Drop the socket immediately; the client sees its reconnect
                // attempt fail and will back off and retry.
                drop(inbound);
                continue;
            }
            let upstream = upstream_addr.clone();
            let task = tokio::spawn(forward(inbound, upstream));
            active.lock().unwrap().push(task.abort_handle());
        }
    });

    (format!("http://127.0.0.1:{local_port}"), controller)
}

/// Pump bytes between `inbound` and a fresh outbound connection to
/// `upstream_addr`. Returns when either direction closes or the task is
/// aborted via `ProxyController::disconnect_all`.
async fn forward(inbound: TcpStream, upstream_addr: String) {
    let outbound = match TcpStream::connect(&upstream_addr).await {
        Ok(s) => s,
        Err(_) => return,
    };

    let (mut ri, mut wi) = inbound.into_split();
    let (mut ro, mut wo) = outbound.into_split();

    let inbound_to_outbound = async {
        let _ = tokio::io::copy(&mut ri, &mut wo).await;
        let _ = wo.shutdown().await;
    };
    let outbound_to_inbound = async {
        let _ = tokio::io::copy(&mut ro, &mut wi).await;
        let _ = wi.shutdown().await;
    };

    tokio::select! {
        _ = inbound_to_outbound => {}
        _ = outbound_to_inbound => {}
    }
}
