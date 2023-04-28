// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{btree_map::Entry, BTreeMap},
    net::Ipv4Addr,
    sync::{Arc, Mutex},
};

use arc_swap::ArcSwap;
use mysten_network::{multiaddr::Protocol, Multiaddr};
use thiserror::Error;
use tracing::info;
use types::{metered_channel::Sender, Transaction, TxResponse};

/// Uses a map to allow running multiple Narwhal instances in the same process.
/// TODO: after Rust 1.66, use BTreeMap::new() instead of wrapping it in an Option.
static LOCAL_NARWHAL_CLIENTS: Mutex<BTreeMap<Multiaddr, Arc<ArcSwap<LocalNarwhalClient>>>> =
    Mutex::new(BTreeMap::new());

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
        let mut clients = LOCAL_NARWHAL_CLIENTS.lock().unwrap();
        match clients.entry(addr) {
            Entry::Vacant(entry) => {
                entry.insert(Arc::new(ArcSwap::from(instance)));
            }
            Entry::Occupied(mut entry) => {
                entry.get_mut().store(instance);
            }
        };
    }

    /// Gets the instance of LocalNarwhalClient for the local address.
    /// Address is only used as the key.
    pub fn get_global(addr: &Multiaddr) -> Option<Arc<ArcSwap<Self>>> {
        let addr = Self::canonicalize_address_key(addr.clone());
        let clients = LOCAL_NARWHAL_CLIENTS.lock().unwrap();
        clients.get(&addr).cloned()
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
