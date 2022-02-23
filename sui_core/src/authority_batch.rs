// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::authority::AuthorityStore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sui_types::base_types::*;
use sui_types::error::{SuiError, SuiResult};

use std::collections::BTreeMap;
use std::time::Duration;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::time::interval;

use typed_store::Map;

#[cfg(test)]
#[path = "unit_tests/batch_tests.rs"]
mod batch_tests;

/*

An authority asynchronously creates batches from its sequence of
certificates / effects. Then both the sequence of certificates
/ effects are transmitted to listeners (as a transaction digest)
as well as batches.

The architecture is as follows:
- The authority store notifies through the Sender that a new
  certificate / effect has been sequenced, at a specific sequence
  number.
- The sender sends this information through a channel to the Manager,
  that decides whether a new batch should be made. This is based on
  time elapsed as well as current size of batch. If so a new batch
  is created.
- The authority manager also holds the sending ends of a number of
  channels that eventually go to clients that registered interest
  in receiving all updates from the authority. When a new item is
  sequenced of a batch created this is sent out to them.

*/

pub type BroadcastPair = (
    tokio::sync::broadcast::Sender<UpdateItem>,
    tokio::sync::broadcast::Receiver<UpdateItem>,
);

/// Either a freshly sequenced transaction hash or a batch
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Debug, Serialize, Deserialize)]
pub enum UpdateItem {
    Transaction((usize, TransactionDigest)),
    Batch(AuthorityBatch),
}

pub struct BatcherSender {
    /// Channel for sending updates.
    tx_send: Sender<(usize, TransactionDigest)>,
}

pub struct BatcherManager {
    /// Channel for receiving updates
    tx_recv: Receiver<(usize, TransactionDigest)>,
    /// The sender end of the broadcast channel used to send updates to listeners
    tx_broadcast: tokio::sync::broadcast::Sender<UpdateItem>,
    /// Copy of the database to write batches and read transactions.
    db: Arc<AuthorityStore>,
}

impl BatcherSender {
    /// Send a new event to the batch manager
    pub async fn send_item(
        &self,
        transaction_sequence: usize,
        transaction_digest: TransactionDigest,
    ) -> Result<(), SuiError> {
        self.tx_send
            .send((transaction_sequence, transaction_digest))
            .await
            .map_err(|_| SuiError::BatchErrorSender)
    }
}

impl BatcherManager {
    pub fn new(
        db: Arc<AuthorityStore>,
        capacity: usize,
    ) -> (BatcherSender, BatcherManager, BroadcastPair) {
        let (tx_send, tx_recv) = channel(capacity);
        let (tx_broadcast, rx_broadcast) = tokio::sync::broadcast::channel(capacity);
        let sender = BatcherSender { tx_send };
        let manager = BatcherManager {
            tx_recv,
            tx_broadcast: tx_broadcast.clone(),
            db,
        };
        (sender, manager, (tx_broadcast, rx_broadcast))
    }

    /// Starts the manager service / tokio task
    pub fn start_service() {}

    async fn init_from_database(&self) -> Result<AuthorityBatch, SuiError> {
        // First read the last batch in the db
        let mut last_batch = match self.db.batches.iter().skip_prior_to(&usize::MAX)?.next() {
            Some((_, last_batch)) => last_batch,
            None => {
                // Make a batch at zero
                let zero_batch = AuthorityBatch {
                    total_size: 0,
                    previous_total_size: 0,
                };
                self.db.batches.insert(&0, &zero_batch)?;
                zero_batch
            }
        };

        // See if there are any transactions in the database not in a batch
        let mut total_seq = self
            .db
            .next_sequence_number
            .load(std::sync::atomic::Ordering::Relaxed);
        if total_seq > last_batch.total_size {
            // Make a new batch, to put the old transactions not in a batch in.
            let transactions: Vec<_> = self
                .db
                .executed_sequence
                .iter()
                .skip_to(&last_batch.total_size)?
                .collect();

            if transactions.len() != total_seq - last_batch.total_size {
                // NOTE: The database is corrupt, namely we have a higher maximum transaction sequence
                //       than number of items, which means there is a hole in the sequence. This can happen
                //       in case we crash after writting command seq x but before x-1 was written. What we
                //       need to do is run the database recovery logic.

                let db_batch = self.db.executed_sequence.batch();

                // Delete all old transactions
                let db_batch = db_batch.delete_batch(
                    &self.db.executed_sequence,
                    transactions.iter().map(|(k, _)| *k),
                )?;

                // Reorder the transactions
                total_seq = last_batch.total_size + transactions.len();
                self.db
                    .next_sequence_number
                    .store(total_seq, std::sync::atomic::Ordering::Relaxed);

                let range = last_batch.total_size..total_seq;
                let db_batch = db_batch.insert_batch(
                    &self.db.executed_sequence,
                    range
                        .into_iter()
                        .zip(transactions.into_iter().map(|(_, v)| v)),
                )?;

                db_batch.write()?;
            }

            last_batch = AuthorityBatch {
                total_size: total_seq,
                previous_total_size: last_batch.total_size,
            };
            self.db.batches.insert(&total_seq, &last_batch)?;
        }

        Ok(last_batch)
    }

    pub async fn run_service(&mut self, min_batch_size: usize, max_delay: Duration) -> SuiResult {
        // We first use the state of the database to establish what the current
        // latest batch is.
        let mut _last_batch = self.init_from_database().await?;

        // Then we operate in a loop, where for each new update we consider
        // whether to create a new batch or not.

        let mut interval = interval(max_delay);
        let mut exit = false;
        let mut make_batch;

        // The structures we use to build the next batch. The current_batch holds the sequence
        // of transactions in order, following the last batch. The loose transactions holds
        // transactions we may have received out of order.
        let (mut current_batch, mut loose_transactions): (
            Vec<(usize, TransactionDigest)>,
            BTreeMap<usize, TransactionDigest>,
        ) = (Vec::new(), BTreeMap::new());
        let mut next_sequence_number = _last_batch.total_size;

        while !exit {
            // Reset the flags.
            make_batch = false;

            // check if we should make a new block
            tokio::select! {
              _ = interval.tick() => {
                // Every so often we check if we should make a batch
                // smaller than the max size. But never empty.
                  make_batch = true;
              },
              item_option = self.tx_recv.recv() => {

                match item_option {
                  None => {
                    make_batch = true;
                    exit = true;
                  },
                  Some((seq, tx_digest)) => {

                    loose_transactions.insert(seq, tx_digest);
                    while loose_transactions.contains_key(&next_sequence_number) {
                      let next_item = (next_sequence_number, loose_transactions.remove(&next_sequence_number).unwrap());
                      // Send the update
                      let _ = self.tx_broadcast.send(UpdateItem::Transaction(next_item));
                      current_batch.push(next_item);
                      next_sequence_number += 1;
                    }

                    if current_batch.len() >= min_batch_size {
                      make_batch = true;
                    }
                  }
                }
               }
            }

            // Logic to make a batch
            if make_batch {
                if current_batch.is_empty() {
                    continue;
                }

                // Make and store a new batch.
                let new_batch = AuthorityBatch {
                    total_size: next_sequence_number,
                    previous_total_size: _last_batch.total_size,
                };
                self.db.batches.insert(&new_batch.total_size, &new_batch)?;

                // Send the update
                let _ = self.tx_broadcast.send(UpdateItem::Batch(new_batch));

                // A new batch is actually made, so we reset the conditions.
                _last_batch = new_batch;
                current_batch.clear();
                interval.reset();
            }
        }

        // When a new batch is created we send a notification to all who have
        // registered an interest.

        Ok(())
    }

    /// Register a sending channel used to send streaming
    /// updates to clients.
    pub fn register_listener() {}
}

#[derive(
    Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Default, Debug, Serialize, Deserialize,
)]
pub struct AuthorityBatch {
    /// The total number of items executed by this authority.
    total_size: usize,

    /// The number of items in the previous block.
    previous_total_size: usize,
    // TODO: Add the following information:
    // - Authenticator of previous block (digest)
    // - Authenticator of this block header + contents (digest)
    // - Signature on block + authenticators
    // - Structures to facilitate sync, eg. IBLT or Merkle Tree.
    // - Maybe: a timestamp (wall clock time)?
}
