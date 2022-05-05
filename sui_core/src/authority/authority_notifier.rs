// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

use std::sync::atomic::{AtomicBool, AtomicU64};
use sui_types::batch::TxSequenceNumber;

use tokio::sync::Notify;
use typed_store::traits::Map;

pub struct TransactionNotifier {
    state: Arc<AuthorityStore>,
    low_watermark: AtomicU64,
    high_watermark: AtomicU64,
    notify: Notify,
    has_stream: AtomicBool,
    is_closed: AtomicBool,
}

impl TransactionNotifier {
    /// Create a new transaction notifier for the authority store
    pub fn new(state: Arc<AuthorityStore>) -> SuiResult<TransactionNotifier> {
        let seq = state.next_sequence_number()?;
        Ok(TransactionNotifier {
            state,
            low_watermark: AtomicU64::new(seq),
            high_watermark: AtomicU64::new(seq),
            notify: Notify::new(),
            has_stream: AtomicBool::new(false),
            is_closed: AtomicBool::new(false),
        })
    }

    pub fn low_watermark(&self) -> TxSequenceNumber {
        self.low_watermark.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Get a ticket with a sequence number
    pub fn ticket(self: &Arc<Self>) -> SuiResult<TransactionNotifierTicket> {
        if self.is_closed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(SuiError::ClosedNotifierError);
        }

        let seq = self
            .high_watermark
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(TransactionNotifierTicket {
            transaction_notifier: self.clone(),
            seq,
        })
    }

    /// Get an iterator, and return an error if an iterator for this stream already exists.
    pub fn iter_from(
        self: &Arc<Self>,
        next_seq: u64,
    ) -> SuiResult<impl futures::Stream<Item = (TxSequenceNumber, TransactionDigest)> + Unpin> {
        if self
            .has_stream
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .is_err()
        {
            return Err(SuiError::ConcurrentIteratorError);
        }

        // The state we inject in the async stream
        let transaction_notifier = self.clone();
        let temp_buffer: VecDeque<(TxSequenceNumber, TransactionDigest)> = VecDeque::new();
        let uniquess_guard = IterUniquenessGuard(transaction_notifier.clone());
        let initial_state = (transaction_notifier, temp_buffer, next_seq, uniquess_guard);

        Ok(Box::pin(futures::stream::unfold(
            initial_state,
            |state| async move {
                let (transaction_notifier, mut temp_buffer, mut next_seq, uniquess_guard) = state;

                loop {
                    // If we have data in the buffer return that first
                    if let Some(item) = temp_buffer.pop_front() {
                        return Some((
                            item,
                            (transaction_notifier, temp_buffer, next_seq, uniquess_guard),
                        ));
                    }

                    // It means we got a notification
                    let last_safe = transaction_notifier
                        .low_watermark
                        .load(std::sync::atomic::Ordering::SeqCst);

                    // Get the stream of updates since the last point we requested ...
                    if let Ok(iter) = transaction_notifier
                        .clone()
                        .state
                        .executed_sequence
                        .iter()
                        .skip_to(&next_seq)
                    {
                        // ... contued here with take_while. And expand the buffer with the new items.
                        temp_buffer.extend(
                            iter.take_while(|(tx_seq, _tx_digest)| *tx_seq < last_safe)
                                .map(|(tx_seq, _tx_digest)| (tx_seq, _tx_digest)),
                        );

                        // Update what the next item would be to no re-read messages in the buffer
                        if !temp_buffer.is_empty() {
                            next_seq = temp_buffer[temp_buffer.len() - 1].0 + 1;
                        }

                        // If we have data in the buffer return that first
                        if let Some(item) = temp_buffer.pop_front() {
                            return Some((
                                item,
                                (transaction_notifier, temp_buffer, next_seq, uniquess_guard),
                            ));
                        } else {
                            // If the notifier is closed, then exit
                            if transaction_notifier
                                .is_closed
                                .load(std::sync::atomic::Ordering::SeqCst)
                            {
                                return None;
                            }
                        }
                    } else {
                        return None;
                    }

                    // Wait for a notification to get more data
                    transaction_notifier.notify.notified().await;
                }
            },
        )))
    }

    /// Signal we want to close this channel.
    pub fn close(&self) {
        self.is_closed
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.notify.notify_one();
    }
}

struct IterUniquenessGuard(Arc<TransactionNotifier>);

impl Drop for IterUniquenessGuard {
    fn drop(&mut self) {
        self.0
            .has_stream
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

pub struct TransactionNotifierTicket {
    transaction_notifier: Arc<TransactionNotifier>,
    seq: u64,
}

impl TransactionNotifierTicket {
    /// Get the ticket sequence number
    pub fn seq(&self) -> u64 {
        self.seq
    }
}

/// A custom drop that indicates that there may not be a item
/// associated with this sequence number,
impl Drop for TransactionNotifierTicket {
    fn drop(&mut self) {
        self.transaction_notifier
            .low_watermark
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.transaction_notifier.notify.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::StreamExt;
    use std::env;
    use std::fs;

    use std::time::Duration;
    use tokio::time::timeout;

    use crate::authority::authority_tests::max_files_authority_tests;

    #[tokio::test]
    async fn test_notifier() {
        let dir = env::temp_dir();
        let path = dir.join(format!("DB_{:?}", ObjectID::random()));
        fs::create_dir(&path).unwrap();

        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_authority_tests());
        let store = Arc::new(AuthorityStore::open(path, Some(opts)));

        let notifier = Arc::new(TransactionNotifier::new(store.clone()).unwrap());

        // TEST 1: Happy sequence

        {
            let t0 = &notifier.ticket().expect("ok");
            store.side_sequence(t0.seq(), &TransactionDigest::random());
        }

        {
            let t0 = &notifier.ticket().expect("ok");
            store.side_sequence(t0.seq(), &TransactionDigest::random());
        }

        {
            let t0 = &notifier.ticket().expect("ok");
            store.side_sequence(t0.seq(), &TransactionDigest::random());
        }

        let mut iter = notifier.iter_from(0).unwrap();

        // Trying to take a second concurrent stream fails.
        assert!(matches!(
            notifier.iter_from(0),
            Err(SuiError::ConcurrentIteratorError)
        ));

        assert!(matches!(iter.next().await, Some((0, _))));
        assert!(matches!(iter.next().await, Some((1, _))));
        assert!(matches!(iter.next().await, Some((2, _))));

        assert!(timeout(Duration::from_millis(10), iter.next())
            .await
            .is_err());

        // TEST 2: Drop a ticket

        {
            let t0 = &notifier.ticket().expect("ok");
            assert!(t0.seq() == 3);
        }

        {
            let t0 = &notifier.ticket().expect("ok");
            store.side_sequence(t0.seq(), &TransactionDigest::random());
        }

        let x = iter.next().await;
        assert!(matches!(x, Some((4, _))));

        assert!(timeout(Duration::from_millis(10), iter.next())
            .await
            .is_err());

        // TEST 3: Drop & out of order

        let t5 = notifier.ticket().expect("ok");
        let t6 = notifier.ticket().expect("ok");
        let t7 = notifier.ticket().expect("ok");
        let t8 = notifier.ticket().expect("ok");

        store.side_sequence(t6.seq(), &TransactionDigest::random());
        drop(t6);

        store.side_sequence(t5.seq(), &TransactionDigest::random());
        drop(t5);

        drop(t7);

        store.side_sequence(t8.seq(), &TransactionDigest::random());
        drop(t8);

        assert!(matches!(iter.next().await, Some((5, _))));
        assert!(matches!(iter.next().await, Some((6, _))));
        assert!(matches!(iter.next().await, Some((8, _))));

        assert!(timeout(Duration::from_millis(10), iter.next())
            .await
            .is_err());

        drop(iter);

        // After we drop an iterator we can get another one
        assert!(notifier.iter_from(0).is_ok());
    }
}
