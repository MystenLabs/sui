// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::*;
use sui_types::batch::*;
use sui_types::error::{SuiError, SuiResult};

use std::time::Duration;
use tokio::time::interval;

use futures::StreamExt;
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
- The authority store notifies that a new certificate / effect has
  been sequenced.
- If the batch service is running it reaches into the notifier and
  finds the highest safe index in the trasaction sequence. An index
  is safe if no task is handling a lower index (they have either
  written to the DB or are dead.)
- The batch service then reads from the database the new items in
  safe sequence, makes batches, writes them to the database,
  and broadcasts them to anyone who is subscribed.
- Only a single batch service is allowed to run at a time. If it
  crashes another can be launched. And that is safe.

*/

pub type BroadcastSender = tokio::sync::broadcast::Sender<UpdateItem>;
pub type BroadcastReceiver = tokio::sync::broadcast::Receiver<UpdateItem>;

pub type BroadcastPair = (BroadcastSender, BroadcastReceiver);

impl crate::authority::AuthorityState {
    /// Initializes the database to handle batches, and recovers from a potential
    /// crash by creating a last batch to include any trailing trasnactions not
    /// in a batch.
    ///
    /// This needs exclusive access to the database at this point, so we take
    /// the authority state as a &mut.
    pub fn init_batches_from_database(&mut self) -> Result<AuthorityBatch, SuiError> {
        // First read the last batch in the db
        let mut last_batch = match self
            .db()
            .batches
            .iter()
            .skip_prior_to(&TxSequenceNumber::MAX)?
            .next()
        {
            Some((_, last_batch)) => last_batch.batch,
            None => {
                // Make a batch at zero
                let zero_batch =
                    SignedBatch::new(AuthorityBatch::initial(), &*self.secret, self.name);
                self.db().batches.insert(&0, &zero_batch)?;
                zero_batch.batch
            }
        };

        // See if there are any transactions in the database not in a batch
        let transactions: Vec<_> = self
            .db()
            .executed_sequence
            .iter()
            .skip_to(&last_batch.next_sequence_number)?
            .collect();

        if !transactions.is_empty() {
            // Make a new batch, to put the old transactions not in a batch in.
            let last_signed_batch = SignedBatch::new(
                AuthorityBatch::make_next(&last_batch, &transactions[..]),
                &*self.secret,
                self.name,
            );
            self.db().batches.insert(
                &last_signed_batch.batch.next_sequence_number,
                &last_signed_batch,
            )?;
            last_batch = last_signed_batch.batch;
        }

        Ok(last_batch)
    }

    pub async fn run_batch_service(
        &self,
        min_batch_size: u64,
        max_delay: Duration,
    ) -> SuiResult<()> {
        // This assumes we have initialized the database with a batch.
        let (next_sequence_number, prev_signed_batch) = self
            .db()
            .batches
            .iter()
            .skip_prior_to(&TxSequenceNumber::MAX)?
            .next()
            .unwrap();

        // Let's ensure we can get (exclusive) access to the trasnaction stream.
        let mut transaction_stream = self.batch_notifier.iter_from(next_sequence_number)?;

        // Then we operate in a loop, where for each new update we consider
        // whether to create a new batch or not.
        let mut interval = interval(max_delay);
        let mut exit = false;
        let mut make_batch;

        let mut prev_batch = prev_signed_batch.batch;

        // The structures we use to build the next batch. The current_batch holds the sequence
        // of transactions in order, following the last batch. The loose transactions holds
        // transactions we may have received out of order.
        let mut current_batch: Vec<(TxSequenceNumber, TransactionDigest)> = Vec::new();

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
              item_option = transaction_stream.next() => {
                match item_option {
                    None => {
                        make_batch = true;
                        exit = true;
                    },
                    Some((seq, tx_digest)) => {
                        // Add to batch and broadcast
                        current_batch.push((seq, tx_digest));
                        let _ = self.batch_channels.send(UpdateItem::Transaction((seq, tx_digest)));

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
                    &*self.secret,
                    self.name,
                );
                self.db()
                    .batches
                    .insert(&new_batch.batch.next_sequence_number, &new_batch)?;

                // Send the update
                let _ = self
                    .batch_channels
                    .send(UpdateItem::Batch(new_batch.clone()));

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
