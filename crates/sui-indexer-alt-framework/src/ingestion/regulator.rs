// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::info;

/// The regulator task is responsible for writing out checkpoint sequence numbers from the
/// `checkpoints` iterator to `checkpoint_tx`, bounded by the high watermark dictated by
/// subscribers.
///
/// Subscribers can share their commit high (exclusive) on `commit_hi_rx`.
/// The regulator remembers these, and stops serving checkpoints if they are over the minimum
/// subscriber commit_hi plus the ingestion `buffer_size`.
///
/// This offers a form of back-pressure that is sensitive to ordering, which is useful for
/// subscribers that need to commit information in order: Without it, those subscribers may need to
/// buffer unboundedly many updates from checkpoints while they wait for the checkpoint that they
/// need to commit.
///
/// Note that back-pressure is optional, and will only be applied if a subscriber provides a
/// commit_hi, at which point it must keep updating the commit_hi to allow the ingestion service to
/// continue making progress.
///
/// The `initial_commit_hi` parameter allows the regulator to start with an initial bound
/// on how far it can ingest. This is useful for scenarios where the indexer is restarted and
/// already has some checkpoints ingested, but subscribers may not yet have reported their commit_hi.
///
/// The task will shut down if the `cancel` token is signalled, or if the `checkpoints` iterator
/// runs out.
pub(super) fn regulator<I>(
    checkpoints: I,
    buffer_size: usize,
    initial_commit_hi: Option<u64>,
    mut commit_hi_rx: mpsc::UnboundedReceiver<(&'static str, u64)>,
    checkpoint_tx: mpsc::Sender<u64>,
    cancel: CancellationToken,
) -> JoinHandle<()>
where
    I: IntoIterator<Item = u64> + Send + Sync + 'static,
    I::IntoIter: Send + Sync + 'static,
{
    tokio::spawn(async move {
        let mut commit_hi = initial_commit_hi;
        let mut subscribers_hi = HashMap::new();
        let mut checkpoints = checkpoints.into_iter().peekable();

        info!("Starting ingestion regulator");

        loop {
            let Some(cp) = checkpoints.peek() else {
                info!("Checkpoints done, stopping regulator");
                break;
            };

            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("Shutdown received, stopping regulator");
                    break;
                }

                // docs::#regulator (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                Some((name, hi)) = commit_hi_rx.recv() => {
                    subscribers_hi.insert(name, hi);
                    commit_hi = subscribers_hi.values().copied().min();
                }
                // docs::/#regulator
                // docs::#bound (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                res = checkpoint_tx.send(*cp), if commit_hi.is_none_or(|hi| *cp < hi + buffer_size as u64) => if res.is_ok() {
                    checkpoints.next();
                } else {
                    info!("Checkpoint channel closed, stopping regulator");
                    break;
                }
                // docs::/#bound
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::{error::Elapsed, timeout};

    use super::*;

    /// Wait up to a second for a response on the channel, and return it, expecting this operation
    /// to succeed.
    async fn expect_recv(rx: &mut mpsc::Receiver<u64>) -> Option<u64> {
        timeout(Duration::from_secs(1), rx.recv()).await.unwrap()
    }

    /// Wait up to a second for a response on the channel, but expecting this operation to timeout.
    async fn expect_timeout(rx: &mut mpsc::Receiver<u64>) -> Elapsed {
        timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn finite_list_of_checkpoints() {
        let (_, commit_hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let cps = 0..5;
        let h_regulator = regulator(cps, 0, None, commit_hi_rx, cp_tx, cancel.clone());

        for i in 0..5 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_on_sender_closed() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let h_regulator = regulator(0.., 0, None, hi_rx, cp_tx, cancel.clone());

        for i in 0..5 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        drop(cp_rx);
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_on_cancel() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let h_regulator = regulator(0.., 0, None, hi_rx, cp_tx, cancel.clone());

        for i in 0..5 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn halted() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        hi_tx.send(("test", 4)).unwrap();

        let h_regulator = regulator(0.., 0, None, hi_rx, cp_tx, cancel.clone());

        for _ in 0..4 {
            expect_recv(&mut cp_rx).await;
        }

        // Regulator stopped because of commit_hi.
        expect_timeout(&mut cp_rx).await;

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn halted_buffered() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        hi_tx.send(("test", 2)).unwrap();

        let h_regulator = regulator(0.., 2, None, hi_rx, cp_tx, cancel.clone());

        for i in 0..4 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Regulator stopped because of commit_hi (plus buffering).
        expect_timeout(&mut cp_rx).await;

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn resumption() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        hi_tx.send(("test", 2)).unwrap();

        let h_regulator = regulator(0.., 0, None, hi_rx, cp_tx, cancel.clone());

        for i in 0..2 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Regulator stopped because of commit_hi, but resumes when that commit_hi is updated.
        expect_timeout(&mut cp_rx).await;
        hi_tx.send(("test", 4)).unwrap();

        for i in 2..4 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Halted again.
        expect_timeout(&mut cp_rx).await;

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn multiple_subscribers() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        hi_tx.send(("a", 2)).unwrap();
        hi_tx.send(("b", 3)).unwrap();

        let cps = 0..10;
        let h_regulator = regulator(cps, 0, None, hi_rx, cp_tx, cancel.clone());

        for i in 0..2 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Regulator stopped because of a's commit_hi.
        expect_timeout(&mut cp_rx).await;

        // Updating b's commit_hi doesn't make a difference.
        hi_tx.send(("b", 4)).unwrap();
        expect_timeout(&mut cp_rx).await;

        // But updating a's commit_hi does.
        hi_tx.send(("a", 3)).unwrap();
        assert_eq!(Some(2), expect_recv(&mut cp_rx).await);

        // ...by one checkpoint.
        expect_timeout(&mut cp_rx).await;

        // And we can make more progress by updating it again.
        hi_tx.send(("a", 4)).unwrap();
        assert_eq!(Some(3), expect_recv(&mut cp_rx).await);

        // But another update to "a" will now not make a difference, because "b" is still behind.
        hi_tx.send(("a", 5)).unwrap();
        expect_timeout(&mut cp_rx).await;

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn test_regulator_with_initial_commit_hi() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        // Start regulator with initial_commit_hi = Some(5) and buffer_size = 0
        let h_regulator = regulator(0.., 0, Some(5), hi_rx, cp_tx, cancel.clone());

        // Regulator should only serve checkpoints 0 through 5 exclusive
        for i in 0..5 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Should be halted at the initial commit_hi
        expect_timeout(&mut cp_rx).await;

        // Sending a subscriber commit_hi should allow progress
        hi_tx.send(("test", 7)).unwrap();

        for i in 5..7 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Halted again
        expect_timeout(&mut cp_rx).await;

        cancel.cancel();
        h_regulator.await.unwrap();
    }
}
