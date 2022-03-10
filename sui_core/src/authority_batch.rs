// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::{AuthorityStore, StableSyncAuthoritySigner};
use std::sync::Arc;
use sui_types::base_types::*;
use sui_types::batch::*;
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

pub type BroadcastSender = tokio::sync::broadcast::Sender<UpdateItem>;
pub type BroadcastReceiver = tokio::sync::broadcast::Receiver<UpdateItem>;

pub type BroadcastPair = (BroadcastSender, BroadcastReceiver);

pub struct BatchSender {
    /// Channel for sending updates.
    tx_send: Sender<(TxSequenceNumber, TransactionDigest)>,
}

pub struct BatchManager {
    /// Channel for receiving updates
    tx_recv: Receiver<(TxSequenceNumber, TransactionDigest)>,
    /// The sender end of the broadcast channel used to send updates to listeners
    tx_broadcast: BroadcastSender,
    /// Copy of the database to write batches and read transactions.
    db: Arc<AuthorityStore>,
}

impl BatchSender {
    /// Send a new event to the batch manager
    pub async fn send_item(
        &self,
        transaction_sequence: TxSequenceNumber,
        transaction_digest: TransactionDigest,
    ) -> Result<(), SuiError> {
        self.tx_send
            .send((transaction_sequence, transaction_digest))
            .await
            .map_err(|_| SuiError::BatchErrorSender)
    }
}

impl BatchManager {
    pub fn new(
        db: Arc<AuthorityStore>,
        capacity: usize,
    ) -> (BatchSender, BatchManager, BroadcastPair) {
        let (tx_send, tx_recv) = channel(capacity);
        let (tx_broadcast, rx_broadcast) = tokio::sync::broadcast::channel(capacity);
        let sender = BatchSender { tx_send };
        let manager = BatchManager {
            tx_recv,
            tx_broadcast: tx_broadcast.clone(),
            db,
        };

        (sender, manager, (tx_broadcast, rx_broadcast))
    }

    /// Starts the manager service / tokio task
    pub async fn start_service(
        mut self,
        authority_name: AuthorityName,
        secret: StableSyncAuthoritySigner,
        min_batch_size: u64,
        max_delay: Duration,
    ) -> Result<tokio::task::JoinHandle<()>, SuiError> {
        let last_batch = self
            .init_from_database(authority_name, secret.clone())
            .await?;

        let join_handle = tokio::spawn(async move {
            self.run_service(
                authority_name,
                secret,
                last_batch,
                min_batch_size,
                max_delay,
            )
            .await
            .expect("Service returns with no errors");
            drop(self);
        });

        Ok(join_handle)
    }

    async fn init_from_database(
        &self,
        authority_name: AuthorityName,
        secret: StableSyncAuthoritySigner,
    ) -> Result<AuthorityBatch, SuiError> {
        // First read the last batch in the db
        let mut last_batch = match self
            .db
            .batches
            .iter()
            .skip_prior_to(&TxSequenceNumber::MAX)?
            .next()
        {
            Some((_, last_batch)) => last_batch.batch,
            None => {
                // Make a batch at zero
                let zero_batch =
                    SignedBatch::new(AuthorityBatch::initial(), &*secret, authority_name);
                self.db.batches.insert(&0, &zero_batch)?;
                zero_batch.batch
            }
        };

        // See if there are any transactions in the database not in a batch
        let transactions: Vec<_> = self
            .db
            .executed_sequence
            .iter()
            .skip_to(&last_batch.next_sequence_number)?
            .collect();

        if !transactions.is_empty() {
            // Make a new batch, to put the old transactions not in a batch in.
            let last_signed_batch = SignedBatch::new(
                AuthorityBatch::make_next(&last_batch, &transactions[..]),
                &*secret,
                authority_name,
            );
            self.db.batches.insert(
                &last_signed_batch.batch.next_sequence_number,
                &last_signed_batch,
            )?;
            last_batch = last_signed_batch.batch;
        }

        Ok(last_batch)
    }

    pub async fn run_service(
        &mut self,
        authority_name: AuthorityName,
        secret: StableSyncAuthoritySigner,
        prev_batch: AuthorityBatch,
        min_batch_size: u64,
        max_delay: Duration,
    ) -> SuiResult {
        // Then we operate in a loop, where for each new update we consider
        // whether to create a new batch or not.

        let mut interval = interval(max_delay);
        let mut exit = false;
        let mut make_batch;

        let mut prev_batch = prev_batch;

        // The structures we use to build the next batch. The current_batch holds the sequence
        // of transactions in order, following the last batch. The loose transactions holds
        // transactions we may have received out of order.
        let mut current_batch: Vec<(TxSequenceNumber, TransactionDigest)> = Vec::new();
        let mut loose_transactions: BTreeMap<TxSequenceNumber, TransactionDigest> = BTreeMap::new();

        let mut next_sequence_number = prev_batch.next_sequence_number;

        while !exit {
            // Reset the flags.
            make_batch = false;

            // check if we should make a new block
            tokio::select! {
              _ = interval.tick() => {
                // Every so often we check if we should make a batch
                // but it should never be empty. But never empty.
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

                    if current_batch.len() as TxSequenceNumber >= min_batch_size {
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
                let new_batch = SignedBatch::new(
                    AuthorityBatch::make_next(&prev_batch, &current_batch),
                    &*secret,
                    authority_name,
                );
                self.db
                    .batches
                    .insert(&new_batch.batch.next_sequence_number, &new_batch)?;

                // Send the update
                let _ = self.tx_broadcast.send(UpdateItem::Batch(new_batch.clone()));

                // A new batch is actually made, so we reset the conditions.
                prev_batch = new_batch.batch;
                current_batch.clear();

                // We rest the interval here to ensure that blocks
                // are made either when they are full or old enough.
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
