// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use prometheus::Registry;
use sui_rpc::Client;
use tonic::transport::Uri;

use crate::metrics::GrpcMetricsLayer;

/// A fixed-size pool of gRPC clients to the ledger service, each a distinct HTTP/2 connection, with
/// calls round-robined across them. One connection caps throughput at what a single HTTP/2
/// connection's flow-control window and driver task can sustain, so the pool lets one replica drive
/// several connections in parallel. Cloning shares one set of connections and one round-robin cursor.
#[derive(Clone)]
pub(crate) struct ClientPool {
    inner: Arc<Inner>,
}

struct Inner {
    clients: Vec<Client>,
    next: AtomicUsize,
}

impl ClientPool {
    /// Build `size` clients (at least one) to `uri`, each sharing the decoding limit, metrics layer,
    /// and optional response-headers timeout.
    pub(crate) fn new(
        uri: Uri,
        size: usize,
        max_decoding_message_size: usize,
        timeout: Option<Duration>,
        prefix: Option<&str>,
        registry: &Registry,
    ) -> anyhow::Result<Self> {
        let metrics = GrpcMetricsLayer::new(prefix.unwrap_or("ledger_grpc"), registry);
        let clients = (0..size.max(1))
            .map(|_| {
                let mut client = Client::new(uri.clone())?
                    .with_max_decoding_message_size(max_decoding_message_size)
                    .request_layer(metrics.clone());
                if let Some(timeout) = timeout {
                    client = client.with_response_headers_timeout(timeout);
                }
                anyhow::Ok(client)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(Self {
            inner: Arc::new(Inner {
                clients,
                next: AtomicUsize::new(0),
            }),
        })
    }

    /// Pick the next client, round-robin, so concurrent calls spread across the connections rather
    /// than all contending on one.
    pub(crate) fn client(&self) -> Client {
        self.inner.clients[self.next_index()].clone()
    }

    /// The index of the connection the next call returns, advancing the round-robin cursor.
    fn next_index(&self) -> usize {
        self.inner.next.fetch_add(1, Ordering::Relaxed) % self.inner.clients.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool(size: usize) -> ClientPool {
        let uri: Uri = "http://127.0.0.1:1".parse().unwrap();
        ClientPool::new(uri, size, 1024, None, None, &Registry::new()).unwrap()
    }

    #[test]
    fn round_robins_and_wraps() {
        let pool = test_pool(3);
        let picks: Vec<usize> = (0..7).map(|_| pool.next_index()).collect();
        assert_eq!(picks, vec![0, 1, 2, 0, 1, 2, 0]);
    }

    #[test]
    fn clones_share_the_round_robin_cursor() {
        let pool = test_pool(3);
        let clone = pool.clone();
        // Interleaving calls across clones keeps advancing one shared cursor.
        assert_eq!(pool.next_index(), 0);
        assert_eq!(clone.next_index(), 1);
        assert_eq!(pool.next_index(), 2);
        assert_eq!(clone.next_index(), 0);
    }

    #[test]
    fn zero_size_is_clamped_to_a_usable_pool() {
        // `size.max(1)` must keep the pool non-empty, or the round-robin modulo/index would panic.
        let pool = test_pool(0);
        assert_eq!(pool.next_index(), 0);
        assert_eq!(pool.next_index(), 0);
    }
}
