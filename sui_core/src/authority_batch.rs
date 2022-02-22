// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::authority::AuthorityStore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sui_types::base_types::*;
use sui_types::error::{SuiError, SuiResult};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use typed_store::Map;

#[cfg(test)]
#[path = "unit_tests/batch_tests.rs"]
mod batch_tests;

/*

An authority asynchronously creates blocks from its sequence of
certificates / effects. Then both the sequence of certificates
/ effects are transmitted to listeners (as a transaction digest)
as well as blocks.

The architecture is as follows:
- The authority store notifies through the Sender that a new
  certificate / effect has been sequenced, at a specific sequence
  number.
- The sender sends this information through a channel to the Manager,
  that decides whether a new block should be made. This is based on
  time elapsed as well as current size of block. If so a new block
  is created.
- The authority manager also holds the sending ends of a number of
  channels that eventually go to clients that registered interest
  in receiving all updates from the authority. When a new item is
  sequenced of a block created this is sent out to them.

*/

/// Either a freshly sequenced transaction hash or a block
pub struct UpdateItem {}

pub struct BatcherSender {
    /// Channel for sending updates.
    tx_send: Sender<(usize, TransactionDigest)>,
}

pub struct BatcherManager {
    /// Channel for receiving updates
    tx_recv: Receiver<(usize, TransactionDigest)>,
    /// Copy of the database to write blocks and read transactions.
    db: Arc<AuthorityStore>,
}

impl BatcherSender {
    /// Send a new event to the block manager
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
    pub fn new(db: Arc<AuthorityStore>, capacity: usize) -> (BatcherSender, BatcherManager) {
        let (tx_send, tx_recv) = channel(capacity);
        let sender = BatcherSender { tx_send };
        let manager = BatcherManager { tx_recv, db };
        (sender, manager)
    }

    /// Starts the manager service / tokio task
    pub fn start_service() {}

    async fn init_from_database(&self) -> Result<AuthorityBatch, SuiError> {
        let mut last_block = match self.db.batches.iter().skip_prior_to(&usize::MAX)?.next() {
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
        let total_seq = self
            .db
            .next_sequence_number
            .load(std::sync::atomic::Ordering::Relaxed);
        if total_seq > last_block.total_size {
            // Make a new batch, to put the old transactions not in a batch in.
            let transactions: Vec<_> = self
                .db
                .executed_sequence
                .iter()
                .skip_to(&last_block.total_size)?
                .collect();

            if transactions.len() != total_seq - last_block.total_size {
                // TODO: The database is corrupt, namely we have a higher maximum transaction sequence
                //       than number of items, which means there is a hole in the sequence. This can happen
                //       in case we crash after writting command seq x but before x-1 was written. What we
                //       need to do is run the database recovery logic.

                return Err(SuiError::StorageCorrupt);
            }

            last_block = AuthorityBatch {
                total_size: total_seq,
                previous_total_size: last_block.total_size,
            };
            self.db.batches.insert(&total_seq, &last_block)?;
        }

        Ok(last_block)
    }

    pub async fn run_service(&self) -> SuiResult {
        // We first use the state of the database to establish what the current
        // latest batch is.
        let _last_block = self.init_from_database().await?;

        // Then we operate in a loop, where for each new update we consider
        // whether to create a new batch or not.

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
