// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// I really want to express some predicates in a natural
// human form rather than the mnimal one - G
#![allow(clippy::nonminimal_bool)]

use sui_types::base_types::*;
use sui_types::batch::*;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::BatchInfoRequest;
use sui_types::messages::BatchInfoResponseItem;

use crate::authority::AuthorityMetrics;
use crate::authority::AuthorityStore;
use crate::scoped_counter;

use futures::stream::{self, Stream};
use futures::StreamExt;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use typed_store::Map;

use tokio::sync::broadcast::{Receiver, Sender};
use tracing::{debug, error};

#[cfg(test)]
#[path = "unit_tests/batch_tests.rs"]
pub(crate) mod batch_tests;

/*

An authority asynchronously creates batches from its sequence of
certificates / effects. Then both the sequence of certificates
/ effects are transmitted to listeners (as a transaction digest)
as well as batches.

The architecture is as follows:
- The authority store notifies that a new certificate / effect has
  been sequenced.
- If the batch service is running it reaches into the notifier and
  finds the highest safe index in the transaction sequence. An index
  is safe if no task is handling a lower index (they have either
  written to the DB or are dead.)
- The batch service then reads from the database the new items in
  safe sequence, makes batches, writes them to the database,
  and broadcasts them to anyone who is subscribed.
- Only a single batch service is allowed to run at a time. If it
  crashes another can be launched. And that is safe.

*/

pub type BroadcastSender = Sender<UpdateItem>;
pub type BroadcastReceiver = Receiver<UpdateItem>;

pub type BroadcastPair = (BroadcastSender, BroadcastReceiver);

impl crate::authority::AuthorityState {
    pub fn last_batch(&self) -> Result<Option<SignedBatch>, SuiError> {
        let last_batch = self
            .db()
            .perpetual_tables
            .batches
            .iter()
            .skip_prior_to(&TxSequenceNumber::MAX)?
            .next()
            .map(|(_, batch)| batch);
        Ok(last_batch)
    }

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
            .perpetual_tables
            .batches
            .iter()
            .skip_prior_to(&TxSequenceNumber::MAX)?
            .next()
        {
            Some((_, last_batch)) => last_batch.into_data(),
            None => {
                // Make a batch at zero
                let zero_batch = SignedBatch::new(
                    self.epoch(),
                    AuthorityBatch::initial(),
                    &*self.secret,
                    self.name,
                );
                self.db().perpetual_tables.batches.insert(&0, &zero_batch)?;
                zero_batch.into_data()
            }
        };

        // See if there are any transactions in the database not in a batch
        let transactions: Vec<_> = self
            .db()
            .perpetual_tables
            .executed_sequence
            .iter()
            .skip_to(&last_batch.next_sequence_number)?
            .collect();

        if !transactions.is_empty() {
            // Make a new batch, to put the old transactions not in a batch in.
            let last_signed_batch = SignedBatch::new(
                self.epoch(),
                // Unwrap safe due to check not empty
                AuthorityBatch::make_next(&last_batch, &transactions)?,
                &*self.secret,
                self.name,
            );
            self.db().perpetual_tables.batches.insert(
                &last_signed_batch.data().next_sequence_number,
                &last_signed_batch,
            )?;
            last_batch = last_signed_batch.into_data();
        }

        Ok(last_batch)
    }

    pub async fn run_batch_service(&self, min_batch_size: u64, max_delay: Duration) {
        loop {
            match self.run_batch_service_once(min_batch_size, max_delay).await {
                Ok(()) => error!("Restarting batch service, which exited without error"),
                Err(e) => {
                    error!("Restarting batch service, which failed with error: {e:?}")
                }
            };
            // Sleep before restart to prevent CPU pegging in case of immediate error.
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    pub async fn run_batch_service_once(
        &self,
        min_batch_size: u64,
        max_delay: Duration,
    ) -> SuiResult<()> {
        debug!("Batch service started");

        let _guard = scoped_counter!(self.metrics, num_batch_service_tasks);

        // This assumes we have initialized the database with a batch.
        let (next_sequence_number, prev_signed_batch) = self
            .db()
            .perpetual_tables
            .batches
            .iter()
            .skip_prior_to(&TxSequenceNumber::MAX)?
            .next()
            .unwrap();

        // Let's ensure we can get (exclusive) access to the transaction stream.
        let mut transaction_stream = self.batch_notifier.iter_from(next_sequence_number)?;

        // Then we operate in a loop, where for each new update we consider
        // whether to create a new batch or not.
        let mut interval = interval(max_delay);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut exit = false;
        let mut make_batch;

        let mut prev_batch = prev_signed_batch.into_data();

        // The structures we use to build the next batch. The current_batch holds the sequence
        // of transactions in order, following the last batch. The loose transactions holds
        // transactions we may have received out of order.
        let mut current_batch: Vec<(TxSequenceNumber, ExecutionDigests)> = Vec::new();

        loop {
            if exit {
                error!("batch service exited!");
                break;
            }
            // Update the running counter
            self.metrics.batch_svc_is_running.inc();

            // Reset the flags.
            make_batch = false;

            // check if we should make a new block
            tokio::select! {
                _ = interval.tick() => {
                    // Every so often we check if we should make a batch
                    // but it should never be empty.
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

                            self.metrics.batch_service_total_tx_broadcasted.inc();
                            self.metrics.batch_service_latest_seq_broadcasted.set(seq as i64);

                            if current_batch.len() as TxSequenceNumber >= min_batch_size {
                                make_batch = true;
                            }
                        }
                    }
                }
            }

            // Logic to make a batch
            if make_batch {
                // Test it is not empty.
                if current_batch.is_empty() {
                    continue;
                }

                // Make and store a new batch.
                let new_batch = SignedBatch::new(
                    self.epoch(),
                    // Unwrap safe since we tested above it is not empty
                    AuthorityBatch::make_next(&prev_batch, &current_batch).unwrap(),
                    &*self.secret,
                    self.name,
                );
                self.db()
                    .perpetual_tables
                    .batches
                    .insert(&new_batch.data().next_sequence_number, &new_batch)?;
                debug!(next_sequence_number=?new_batch.data().next_sequence_number, "New batch created. Transactions: {:?}", current_batch);

                // Send the update
                let _ = self
                    .batch_channels
                    .send(UpdateItem::Batch(new_batch.clone()));

                // A new batch is actually made, so we reset the conditions.
                prev_batch = new_batch.into_data();
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

    pub async fn handle_batch_streaming(
        &self,
        request: BatchInfoRequest,
    ) -> Result<impl Stream<Item = Result<BatchInfoResponseItem, SuiError>>, SuiError> {
        let metrics = self.metrics.clone();
        metrics.follower_connections.inc();

        let follower_connections_concurrent_guard =
            scoped_counter!(metrics, follower_connections_concurrent);

        metrics
            .follower_start_seq_num
            .observe(request.start.unwrap_or(0) as f64);

        // If we do not have a start, pick next sequence number that has
        // not yet been put into a batch.
        let start = match request.start {
            Some(start) => start,
            None => {
                self.last_batch()?
                    .expect("Authority is always initialized with a batch")
                    .data()
                    .next_sequence_number
            }
        };

        let end = start + request.length;

        let batch = self
            .db()
            .perpetual_tables
            .batches
            .iter()
            .skip_prior_to(&start)
            .map(|mut o| o.next())
            .unwrap_or_default();

        // We could not even find batch 0, return error
        if batch.is_none() {
            return Err(SuiError::BatchErrorSender);
        }

        let (seq, signed_batch) = batch.unwrap();

        #[derive(PartialEq)]
        enum NextItemToPublish {
            Transaction,
            Batch,
        }

        // Define a local structure to support the stream construction.
        struct BatchStreamingLocals<GuardT> {
            _guard: GuardT,
            no_more_txns: bool,
            txns: VecDeque<(TxSequenceNumber, ExecutionDigests)>,
            // Next batch to be read from db should have its next_sequence_number
            // be at least this much
            next_batch_seq: TxSequenceNumber,
            // Read txn from db if this is true or a batch otherwise
            next_item: NextItemToPublish,
            // If none, no more batches will be published to the stream
            // i.e. only trailing transactions will be published and stream
            // will be closed
            pending_batch: Option<SignedBatch>,
            db: Arc<AuthorityStore>,
            metrics: Arc<AuthorityMetrics>,
        }

        let local_state = BatchStreamingLocals {
            _guard: follower_connections_concurrent_guard,
            no_more_txns: false,
            txns: VecDeque::new(),
            next_batch_seq: seq + 1,
            next_item: NextItemToPublish::Batch,
            pending_batch: Some(signed_batch),
            db: self.db(),
            metrics,
        };

        // Construct the stream
        let stream1 = stream::unfold(local_state, move |mut local_state| async move {
            loop {
                if local_state.pending_batch.is_some()
                    && local_state.next_item == NextItemToPublish::Batch
                {
                    // We already published all txns for this pending batch, now is the time to
                    // publish the batch
                    let batch = local_state.pending_batch.unwrap();
                    local_state.pending_batch = None;
                    local_state.metrics.follower_batches_streamed.inc();
                    return Some((
                        Ok(BatchInfoResponseItem(UpdateItem::Batch(batch))),
                        local_state,
                    ));
                } else if local_state.next_item == NextItemToPublish::Transaction {
                    let tx = local_state.txns.pop_front();
                    if let Some((seq, digest)) = tx {
                        local_state.metrics.follower_txes_streamed.inc();
                        return Some((
                            Ok(BatchInfoResponseItem(UpdateItem::Transaction((
                                seq, digest,
                            )))),
                            local_state,
                        ));
                    } else {
                        local_state.next_item = NextItemToPublish::Batch;
                    }
                } else {
                    if local_state.no_more_txns {
                        return None;
                    }
                    let batch = local_state
                        .db
                        .perpetual_tables
                        .batches
                        .iter()
                        .skip_to(&local_state.next_batch_seq)
                        .map(|mut o| o.next())
                        .unwrap_or_default();
                    if let Some((_, signed_batch)) = batch {
                        // we found a new batch
                        let initial_seq_num = signed_batch.data().initial_sequence_number;
                        let next_seq_num = signed_batch.data().next_sequence_number;
                        if let Ok(iter) = local_state
                            .db
                            .perpetual_tables
                            .executed_sequence
                            .iter()
                            .skip_to(&initial_seq_num)
                        {
                            local_state.txns =
                                iter.take_while(|(seq, _)| seq < &next_seq_num).collect();
                        } else {
                            // We failed to read the next txn, close the stream
                            error!("Failed to read txn from db, closing stream");
                            return None;
                        }
                        if initial_seq_num < end {
                            local_state.pending_batch = Some(signed_batch);
                        } else {
                            local_state.pending_batch = None;
                        }
                        if next_seq_num >= end {
                            local_state.no_more_txns = true;
                        }
                        local_state.next_item = NextItemToPublish::Transaction;
                        local_state.next_batch_seq = next_seq_num + 1;
                    } else {
                        // sleep and come back to check if the next batch is ready
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        continue;
                    }
                }
            }
        });

        Ok(stream1)
    }
}
