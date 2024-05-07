// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::{ArcSwap, ArcSwapOption};
use mysten_metrics::metered_channel::Sender;
use mysten_network::{multiaddr::Protocol, Multiaddr};
use std::{
    collections::{btree_map::Entry, BTreeMap},
    net::Ipv4Addr,
    sync::{Arc, Mutex},
};
use thiserror::Error;
use tokio::time::{sleep, timeout, Duration};
use tracing::info;
use types::{Transaction, TxResponse};

#[cfg(msim)]
mod static_client_cache {
    use super::*;
    thread_local! {
        /// Uses a map to allow running multiple Narwhal instances in the same process.
        static LOCAL_NARWHAL_CLIENTS: Mutex<BTreeMap<Multiaddr, Arc<ArcSwap<LocalNarwhalClient>>>> =
            Mutex::new(BTreeMap::new());
    }

    pub(super) fn with_clients<T>(
        f: impl FnOnce(&mut BTreeMap<Multiaddr, Arc<ArcSwap<LocalNarwhalClient>>>) -> T,
    ) -> T {
        LOCAL_NARWHAL_CLIENTS.with(|clients| {
            let mut clients = clients.lock().unwrap();
            f(&mut clients)
        })
    }
}

#[cfg(not(msim))]
mod static_client_cache {
    use super::*;
    /// Uses a map to allow running multiple Narwhal instances in the same process.
    static LOCAL_NARWHAL_CLIENTS: Mutex<BTreeMap<Multiaddr, Arc<ArcSwap<LocalNarwhalClient>>>> =
        Mutex::new(BTreeMap::new());

    pub(super) fn with_clients<T>(
        f: impl FnOnce(&mut BTreeMap<Multiaddr, Arc<ArcSwap<LocalNarwhalClient>>>) -> T,
    ) -> T {
        let mut clients = LOCAL_NARWHAL_CLIENTS.lock().unwrap();
        f(&mut clients)
    }
}

/// The maximum allowed size of transactions into Narwhal.
/// TODO: maybe move to TxValidator?
pub const MAX_ALLOWED_TRANSACTION_SIZE: usize = 6 * 1024 * 1024;

/// Errors returned to clients submitting transactions to Narwhal.
#[derive(Clone, Debug, Error)]
pub enum NarwhalError {
    #[error("Failed to include transaction in a header!")]
    TransactionNotIncludedInHeader,

    #[error("Narwhal is shutting down!")]
    ShuttingDown,

    #[error("Transaction is too large: size={0} limit={1}")]
    TransactionTooLarge(usize, usize),
}

/// TODO: add NarwhalClient trait and implement RemoteNarwhalClient with grpc.

/// A Narwhal client that instantiates LocalNarwhalClient lazily.
pub struct LazyNarwhalClient {
    /// Outer ArcSwapOption allows initialization after the first connection to Narwhal.
    /// Inner ArcSwap allows Narwhal restarts across epoch changes.
    pub client: ArcSwapOption<ArcSwap<LocalNarwhalClient>>,
    pub addr: Multiaddr,
}

impl LazyNarwhalClient {
    /// Lazily instantiates LocalNarwhalClient keyed by the address of the Narwhal worker.
    pub fn new(addr: Multiaddr) -> Self {
        Self {
            client: ArcSwapOption::empty(),
            addr,
        }
    }

    pub async fn get(&self) -> Arc<ArcSwap<LocalNarwhalClient>> {
        // Narwhal may not have started and created LocalNarwhalClient, so retry in a loop.
        // Retries should only happen on Sui process start.
        const NARWHAL_WORKER_START_TIMEOUT: Duration = Duration::from_secs(30);
        if let Ok(client) = timeout(NARWHAL_WORKER_START_TIMEOUT, async {
            loop {
                match LocalNarwhalClient::get_global(&self.addr) {
                    Some(c) => return c,
                    None => {
                        sleep(Duration::from_millis(100)).await;
                    }
                };
            }
        })
        .await
        {
            return client;
        }
        panic!(
            "Timed out after {:?} waiting for Narwhal worker ({}) to start!",
            NARWHAL_WORKER_START_TIMEOUT, self.addr,
        );
    }
}

/// A client that connects to Narwhal locally.
#[derive(Clone)]
pub struct LocalNarwhalClient {
    /// TODO: maybe use tx_batch_maker for load schedding.
    tx_batch_maker: Sender<(Transaction, TxResponse)>,
}

impl LocalNarwhalClient {
    pub fn new(tx_batch_maker: Sender<(Transaction, TxResponse)>) -> Arc<Self> {
        Arc::new(Self { tx_batch_maker })
    }

    /// Sets the instance of LocalNarwhalClient for the local address.
    /// Address is only used as the key.
    pub fn set_global(addr: Multiaddr, instance: Arc<Self>) {
        info!("Narwhal worker client added ({})", addr);
        let addr = Self::canonicalize_address_key(addr);
        static_client_cache::with_clients(|clients| {
            match clients.entry(addr) {
                Entry::Vacant(entry) => {
                    entry.insert(Arc::new(ArcSwap::from(instance)));
                }
                Entry::Occupied(mut entry) => {
                    entry.get_mut().store(instance);
                }
            };
        });
    }

    /// Gets the instance of LocalNarwhalClient for the local address.
    /// Address is only used as the key.
    pub fn get_global(addr: &Multiaddr) -> Option<Arc<ArcSwap<Self>>> {
        let addr = Self::canonicalize_address_key(addr.clone());
        static_client_cache::with_clients(|clients| clients.get(&addr).cloned())
    }

    /// Submits a transaction to the local Narwhal worker.
    pub async fn submit_transaction(&self, transaction: Transaction) -> Result<(), NarwhalError> {
        if transaction.len() > MAX_ALLOWED_TRANSACTION_SIZE {
            return Err(NarwhalError::TransactionTooLarge(
                transaction.len(),
                MAX_ALLOWED_TRANSACTION_SIZE,
            ));
        }
        // Send the transaction to the batch maker.
        let (notifier, when_done) = tokio::sync::oneshot::channel();
        self.tx_batch_maker
            .send((transaction, notifier))
            .await
            .map_err(|_| NarwhalError::ShuttingDown)?;

        let _digest = when_done
            .await
            .map_err(|_| NarwhalError::TransactionNotIncludedInHeader)?;

        Ok(())
    }

    /// Ensures getter and setter use the same key for the same network address.
    /// This is needed because TxServer serves from 0.0.0.0.
    fn canonicalize_address_key(address: Multiaddr) -> Multiaddr {
        address
            .replace(0, |_protocol| Some(Protocol::Ip4(Ipv4Addr::UNSPECIFIED)))
            .unwrap()
    }
}
