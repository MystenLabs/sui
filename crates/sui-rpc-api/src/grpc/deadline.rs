// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::Stream;
use futures::StreamExt;
use futures::stream::BoxStream;
use tokio::time::Instant;

/// Wrap a server-streaming response with a wall-clock deadline.
///
/// Guarantee: when the deadline fires, the inner stream is dropped and
/// its resources (Bigtable permits, in-flight RPCs, render buffers,
/// blocking scan workers) are released in real time — even if the gRPC
/// consumer has stopped pulling frames. The `DeadlineExceeded` Status
/// itself is delivered on the next poll from tonic, which may be later if
/// the h2 send window is closed.
///
/// The naive design — race deadline and `inner.next()` in a single
/// `select!` inside the wrapper — fails when tonic's task is parked at
/// its h2-write await: timer wakes hit the task but resume at the wrong
/// await point, and the wrapper's select never runs. Spawning gives the
/// drain loop its own task whose only outer await is `timeout_at(...)`,
/// so deadline wakes always land where they can cancel.
///
/// The mpsc(1) channel is just the bridge between two polling roots
/// (tonic ↔ our spawn). Capacity 1 = tightest backpressure; per-item
/// wake overhead is negligible against IO/render cost.
///
/// Shared by both the fullnode (`sui-rpc-api`) and bigtable (`sui-kv-rpc`)
/// ledger-history streaming services so they enforce deadlines identically.
pub fn with_deadline<S, T>(
    stream: S,
    deadline: Instant,
    operation: &'static str,
) -> BoxStream<'static, Result<T, tonic::Status>>
where
    S: Stream<Item = Result<T, tonic::Status>> + Send + 'static,
    T: Send + 'static,
{
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<T, tonic::Status>>(1);

    // Spawn the drain loop. `timeout_at` is the outermost await of the
    // task, so a deadline wake always lands inside it and observes
    // Ready, regardless of which inner await (next / send) is suspended.
    let producer = tokio::spawn(async move {
        let _ = tokio::time::timeout_at(deadline, async move {
            futures::pin_mut!(stream);
            while let Some(item) = stream.next().await {
                // Consumer dropped → channel closed → stop work.
                if tx.send(item).await.is_err() {
                    return;
                }
            }
        })
        .await;
    });

    // Synchronous abort if the wrapper is dropped before its body runs
    // (or while suspended inside `inner.next()` rather than `send`).
    struct AbortOnDrop(tokio::task::AbortHandle);
    impl Drop for AbortOnDrop {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let abort_guard = AbortOnDrop(producer.abort_handle());

    // Lift the select! result into an enum so the macro's elaboration
    // can infer the try_stream's error type from the match arms.
    enum Step<T> {
        Item(Option<Result<T, tonic::Status>>),
        Deadline,
    }

    async_stream::try_stream! {
        // Move the guard into the generator so dropping the unpolled
        // stream still drops it (and thus aborts the producer).
        let _abort_on_drop = abort_guard;
        // Held until the consumer drains the channel — then awaited to
        // surface any panic from the producer task as an Internal error.
        let mut producer = producer;
        let sleep = tokio::time::sleep_until(deadline);
        futures::pin_mut!(sleep);
        loop {
            // `biased`: past-deadline polls emit DeadlineExceeded
            // promptly without waiting on a buffered item.
            let step = tokio::select! {
                biased;
                _ = &mut sleep => Step::Deadline,
                item = rx.recv() => Step::Item(item),
            };
            match step {
                Step::Item(Some(Ok(it))) => yield it,
                Step::Item(Some(Err(e))) => Err(e)?,
                Step::Item(None) => {
                    // Producer closed the channel — either natural EOF or
                    // a panic that aborted the task before EOF. Distinguish
                    // by awaiting the JoinHandle: a panic surfaces as an
                    // Internal error so the consumer doesn't see truncated
                    // success. The panic message itself is logged by the
                    // global `telemetry-subscribers` panic hook (the boxed
                    // payload here is an opaque Rust-internal type that
                    // can't be cheaply downcast to a string), so the wire
                    // status carries only a generic marker.
                    //
                    // TODO: once these services add a CatchPanicLayer to
                    // their Tower stack (sister services already do), this
                    // translation can move there and we can just
                    // `resume_unwind` here.
                    match (&mut producer).await {
                        Ok(()) => break,
                        Err(e) if e.is_panic() => {
                            tracing::error!(operation, "producer task panicked");
                            Err(tonic::Status::internal(format!(
                                "{operation} request panicked"
                            )))?;
                        }
                        Err(_) => {
                            // Cancellation — only possible if the abort
                            // guard fired (which only happens on Drop), so
                            // we shouldn't be polling. Treat as EOF.
                            break;
                        }
                    }
                }
                Step::Deadline => {
                    tracing::warn!(operation, "request deadline exceeded");
                    Err(tonic::Status::deadline_exceeded(format!(
                        "{operation} request deadline exceeded"
                    )))?;
                }
            }
        }
    }
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicU64;
    use std::sync::atomic::Ordering as AtomicOrdering;
    use std::time::Duration;

    /// `start_paused = true` makes `tokio::time` virtual: sleeps and
    /// `Instant::now()` advance only when the runtime explicitly waits, so
    /// the test runs instantly and deterministically.
    #[tokio::test(start_paused = true)]
    async fn with_deadline_emits_deadline_exceeded_when_inner_hangs() {
        let inner: BoxStream<'static, Result<u64, tonic::Status>> = stream::pending().boxed();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut bounded = with_deadline(inner, deadline, "test");

        let item = bounded.next().await;
        let status = item.expect("got an item").expect_err("got a status error");
        assert_eq!(status.code(), tonic::Code::DeadlineExceeded);
        assert!(bounded.next().await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn with_deadline_passes_items_through_until_deadline() {
        let inner = stream::iter([Ok::<_, tonic::Status>(1), Ok(2)]).boxed();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut bounded = with_deadline(inner, deadline, "test");

        assert_eq!(bounded.next().await.unwrap().unwrap(), 1);
        assert_eq!(bounded.next().await.unwrap().unwrap(), 2);
        assert!(bounded.next().await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn with_deadline_propagates_inner_error_before_deadline() {
        let inner = stream::iter([Err::<u64, _>(tonic::Status::unavailable("nope"))]).boxed();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut bounded = with_deadline(inner, deadline, "test");

        let status = bounded.next().await.unwrap().unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unavailable);
    }

    /// Counts inner-stream yields to prove the spawned producer is dropped
    /// at deadline-time even when the consumer never polls. If the deadline
    /// were only observed on the consumer's next poll, the counter would
    /// keep growing while virtual time advances. Defends the contract that
    /// `with_deadline` cancels in-flight work in wall-clock time regardless
    /// of consumer pace.
    #[tokio::test(start_paused = true)]
    async fn with_deadline_drops_inner_when_consumer_is_slow_past_deadline() {
        let count = Arc::new(AtomicU64::new(0));
        let inner: BoxStream<'static, Result<u64, tonic::Status>> = {
            let count = count.clone();
            stream::unfold((), move |()| {
                let count = count.clone();
                async move {
                    count.fetch_add(1, AtomicOrdering::SeqCst);
                    Some((Ok::<u64, tonic::Status>(1), ()))
                }
            })
            .boxed()
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut bounded = with_deadline(inner, deadline, "test");

        // Drain one item; producer is now blocked on a full channel
        // somewhere past it.
        assert_eq!(bounded.next().await.unwrap().unwrap(), 1);

        // Stop polling. Advance virtual time past the deadline. The
        // producer-side `timeout_at` must drop the inner stream here.
        tokio::time::sleep(Duration::from_secs(10)).await;
        let snapshot = count.load(AtomicOrdering::SeqCst);

        // Past-deadline: no further inner-stream yields should be observed.
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(
            count.load(AtomicOrdering::SeqCst),
            snapshot,
            "inner stream kept producing past the deadline",
        );

        // And the consumer sees `DeadlineExceeded` on its next poll.
        let status = bounded.next().await.unwrap().unwrap_err();
        assert_eq!(status.code(), tonic::Code::DeadlineExceeded);
    }

    /// A panic inside the producer task must surface as `Internal`
    /// instead of silently closing the channel (which the consumer can't
    /// distinguish from a clean EOF). Without this translation, a
    /// truncated response looks like a successful one to the client.
    #[tokio::test(start_paused = true)]
    async fn with_deadline_translates_producer_panic_to_internal() {
        let inner: BoxStream<'static, Result<u64, tonic::Status>> =
            stream::unfold(0u64, |i| async move {
                if i == 1 {
                    panic!("boom from inner stream");
                }
                Some((Ok::<u64, tonic::Status>(i), i + 1))
            })
            .boxed();
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut bounded = with_deadline(inner, deadline, "test");

        // First item flows through normally.
        assert_eq!(bounded.next().await.unwrap().unwrap(), 0);
        // Next pull observes the producer panic via the JoinHandle.
        let status = bounded.next().await.unwrap().unwrap_err();
        assert_eq!(status.code(), tonic::Code::Internal);
    }

    /// Dropping the wrapper before any item is drained must abort the
    /// spawned producer and drop the inner stream — otherwise a client
    /// that hangs up early would leak the in-flight pipeline until its
    /// own deadline.
    #[tokio::test(start_paused = true)]
    async fn with_deadline_aborts_producer_when_consumer_drops() {
        struct DropBeacon(Arc<AtomicBool>);
        impl Drop for DropBeacon {
            fn drop(&mut self) {
                self.0.store(true, AtomicOrdering::SeqCst);
            }
        }

        let dropped = Arc::new(AtomicBool::new(false));
        let beacon = DropBeacon(dropped.clone());
        // Stream that never yields but holds the beacon for its lifetime —
        // beacon's Drop fires iff the producer task drops the inner stream.
        let inner: BoxStream<'static, Result<u64, tonic::Status>> =
            stream::unfold(beacon, |state| async move {
                let _hold = &state;
                std::future::pending::<()>().await;
                Some((Ok::<u64, tonic::Status>(1), state))
            })
            .boxed();
        let deadline = Instant::now() + Duration::from_secs(60);

        let bounded = with_deadline(inner, deadline, "test");
        drop(bounded);

        // Give the runtime cycles to deliver the abort and drop the
        // producer task's future.
        for _ in 0..10 {
            tokio::task::yield_now().await;
            if dropped.load(AtomicOrdering::SeqCst) {
                break;
            }
        }
        assert!(
            dropped.load(AtomicOrdering::SeqCst),
            "inner stream was not dropped after consumer dropped the wrapper",
        );
    }
}
