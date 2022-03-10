// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_autoinc_channel::Ticket;
use crate::authority::{AuthorityStore, StableSyncAuthoritySigner};
use std::sync::Arc;
use sui_types::base_types::*;
use sui_types::batch::*;
use sui_types::error::{SuiError, SuiResult};

use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio::time::interval;

use typed_store::Map;

use crate::authority::authority_autoinc_channel::AutoIncSender;

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
    pub(crate) autoinc: AutoIncSender<TransactionDigest>,
}

pub struct BatchManager {
    /// Channel for receiving updates
    tx_recv: UnboundedReceiver<(TxSequenceNumber, TransactionDigest)>,
    /// The sender end of the broadcast channel used to send updates to listeners
    tx_broadcast: BroadcastSender,
    /// Copy of the database to write batches and read transactions.
    db: Arc<AuthorityStore>,
}

impl BatchSender {
    /// Send a new event to the batch manager
    pub fn ticket(&self) -> Ticket<TransactionDigest> {
        self.autoinc.next_ticket()
    }
}

impl BatchManager {
    pub fn new(
        db: Arc<AuthorityStore>,
        capacity: usize,
    ) -> Result<(BatchSender, BatchManager, BroadcastPair), SuiError> {
        let (tx_send, tx_recv) = unbounded_channel();
        let (tx_broadcast, rx_broadcast) = tokio::sync::broadcast::channel(capacity);
        let latest_sequence_number = db.next_sequence_number()?;
        let sender = BatchSender {
            autoinc: AutoIncSender::new(tx_send, latest_sequence_number),
        };
        let manager = BatchManager {
            tx_recv,
            tx_broadcast: tx_broadcast.clone(),
            db,
        };

        Ok((sender, manager, (tx_broadcast, rx_broadcast)))
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
        let mut next_expected_sequence_number = prev_batch.next_sequence_number;

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

                    if seq > next_expected_sequence_number {
                        /* We are missing a transaction sequence number, which may be due to
                           an AuthorityState instance crashing AFTER it stored a batch. This is
                           not common, but we go to the DB to see if we can recover the Txs.

                           If the store is dead (which could be the cause of the out of sequence)
                           we simply stop the batch maker. And hope someone restarts it eventually.
                        */

                        self.db.executed_sequence
                            .iter()
                            .skip_to(&next_expected_sequence_number)?
                            .take_while(|(store_seq, _)| store_seq < &seq)
                            .for_each(|(store_seq, store_digest)|
                        {
                                                // Add to batch and broadcast
                            current_batch.push((store_seq, store_digest));
                            let _ = self.tx_broadcast.send(UpdateItem::Transaction((store_seq, store_digest)));

                        });

                    }

                    // Add to batch and broadcast
                    current_batch.push((seq, tx_digest));
                    let _ = self.tx_broadcast.send(UpdateItem::Transaction((seq, tx_digest)));
                    next_expected_sequence_number = seq + 1;

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
}
