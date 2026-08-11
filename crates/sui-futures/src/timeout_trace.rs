// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Wraps a future with a timeout that also captures a [`SpanTrace`] of every await point that
//! was still pending when the deadline fired, by piggy-backing on the [`Waker`] contract.
//!
//! ## The technique
//!
//! Any `Future::poll` that returns [`Poll::Pending`] is required, by the `Waker` contract, to
//! arrange for its task to be polled again later -- almost always by cloning the `Waker` it was
//! given and stashing the clone somewhere (a timer wheel, an I/O reactor's readiness slot, a
//! channel's waiter list, ...) so it can be woken once whatever it's waiting on is ready.
//!
//! [`timeout`] exploits this. Once its deadline elapses, it polls the wrapped future exactly one
//! more time using a custom [`Waker`] (backed by a hand-rolled [`RawWakerVTable`], see
//! `TracingWaker`) that captures a [`SpanTrace`] every time it's cloned. Each clone marks one
//! distinct still-pending await point -- there can be more than one, e.g. when the future
//! resolves several sibling branches concurrently. If that final poll is still `Pending`, every
//! trace captured during it (for clones that are still alive by the time we look) is returned
//! via [`TimeoutElapsed::active_traces`].
//!
//! Because this technique is only ever used for a single, final poll -- whatever it returns,
//! [`timeout`] is done, either way (see [`TracedTimeout::poll`]) -- `TracingWaker` never
//! forwards real wake-ups anywhere. There's no task left that a later wake could usefully
//! re-poll: by the time any retained clone is woken, `timeout`'s own future has either already
//! resolved and been dropped, or is about to resolve on its own regardless.

use std::error::Error;
use std::fmt::Display;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Weak;
use std::task::Context;
use std::task::Poll;
use std::task::RawWaker;
use std::task::RawWakerVTable;
use std::task::Waker;
use std::time::Duration;

use pin_project::pin_project;
use tokio::sync::mpsc;
use tokio::time::Sleep;
use tracing_error::SpanTrace;

/// Wraps a future with a timeout that also captures [`SpanTrace`]s of whatever was still pending
/// when it fired -- see [`timeout`].
#[pin_project]
pub struct TracedTimeout<Fut> {
    #[pin]
    deadline: Sleep,
    #[pin]
    inner: Fut,
}

/// Returned when `inner` hasn't finished within `duration`, carrying a [`SpanTrace`] for every
/// await point of `inner` that was still pending at that moment. There can be more than one --
/// e.g. when a caller resolves several sibling branches concurrently.
#[derive(Debug)]
pub struct TimeoutElapsed {
    pub active_traces: Vec<SpanTrace>,
}

/// One trace captured by a [`TracingWaker`] clone, in transit through the channel described
/// there.
struct TracedAwaitPoint {
    trace: SpanTrace,
    /// Upgrades for as long as the [`TracingWaker`] clone that produced this entry is still
    /// alive; see that type's docs for why that matters.
    alive: Weak<()>,
}

/// A `Waker` that captures a [`SpanTrace`] every time it's cloned -- per the module docs, each
/// clone marks a distinct still-pending await point -- and does nothing else: `wake`,
/// `wake_by_ref`, and `drop` only free the clone's own allocation, forwarding no wake-up to
/// anything. That's sound here specifically because a `TracingWaker` is only ever handed to a
/// single, final poll after `timeout`'s deadline has already elapsed (see
/// [`TracedTimeout::poll`]): that poll's result is final regardless of what it is, so there's
/// never a task left for a later wake-up to usefully reach.
struct TracingWaker {
    /// Traces are sent, one per clone, down this channel; `TracedTimeout::poll` drains it after
    /// the final poll returns. Unbounded: sends only ever happen synchronously during the one
    /// poll call a `TracingWaker` is used for, and are always fully drained immediately
    /// afterward, so there's no producer that could outpace a slow consumer -- the usual reason
    /// the repo disallows `mpsc::unbounded_channel` (see the `#[allow]` where this is
    /// constructed).
    sender: mpsc::UnboundedSender<TracedAwaitPoint>,
    /// Kept alive for as long as this clone is -- e.g. by whatever external structure (a timer,
    /// a connection pool's waiter list, ...) retained the `Waker` clone wrapping it (never read
    /// directly -- `TracedAwaitPoint::alive` observes it through the paired `Weak` handle).
    /// Paired with a `Weak` handle sent alongside this clone's trace, so a trace only "counts" as
    /// an active await point if the clone that produced it is still alive by drain time: a clone
    /// that's created and then immediately dropped without being retained anywhere represents a
    /// future that merely touched its waker without truly registering to be woken by it, and
    /// shouldn't be reported as a pending await point. `None` for the original (root) waker,
    /// which isn't itself an await point and never produces a trace.
    _alive: Option<Arc<()>>,
}

impl TracingWaker {
    const VTABLE: RawWakerVTable = RawWakerVTable::new(
        Self::raw_clone,
        Self::raw_wake,
        Self::raw_wake_by_ref,
        Self::raw_drop,
    );

    fn new_std_waker(sender: mpsc::UnboundedSender<TracedAwaitPoint>) -> Waker {
        let data = Box::into_raw(Box::new(Self {
            sender,
            _alive: None,
        }));
        // SAFETY: `data` was just obtained from `Box::into_raw`, so it's a unique, valid pointer
        // to a `Box<Self>` -- the precondition every vtable function below relies on.
        unsafe { Waker::new(data as *const (), &Self::VTABLE) }
    }

    fn clone_capturing_trace(&self) -> Box<Self> {
        let alive = Arc::new(());
        // Ignore errors: the only way `send` fails here is a closed receiver, meaning
        // `TracedTimeout::poll` has already drained and returned -- there's nowhere useful to put
        // this trace. Dropping it here is never unsafe, only lossy: the timeout still resolves
        // and its response still goes out fine either way, just possibly missing this one entry
        // from the trace list.
        let _ = self.sender.send(TracedAwaitPoint {
            trace: SpanTrace::capture(),
            alive: Arc::downgrade(&alive),
        });
        Box::new(Self {
            sender: self.sender.clone(),
            _alive: Some(alive),
        })
    }

    unsafe fn raw_clone(data: *const ()) -> RawWaker {
        // SAFETY: see `new_std_waker`.
        let this = unsafe { &*(data as *const Self) };
        let cloned = this.clone_capturing_trace();
        RawWaker::new(Box::into_raw(cloned) as *const (), &Self::VTABLE)
    }

    unsafe fn raw_wake(data: *const ()) {
        // No wake-up to forward -- see the type docs -- so `wake` reduces to freeing the box,
        // exactly like `drop` does.
        unsafe { Self::raw_drop(data) };
    }

    unsafe fn raw_wake_by_ref(_data: *const ()) {
        // Nothing to notify; see the type docs.
    }

    unsafe fn raw_drop(data: *const ()) {
        // SAFETY: see `new_std_waker`.
        drop(unsafe { Box::<Self>::from_raw(data as *mut Self) });
    }
}

impl<Fut: Future> Future for TracedTimeout<Fut> {
    type Output = Result<Fut::Output, TimeoutElapsed>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        if this.deadline.poll(cx).is_pending() {
            return this.inner.poll(cx).map(Ok);
        }

        // We hit the timeout. Do one final poll of `inner`, capturing a trace every time it
        // clones its waker. This call resolves this whole `poll` one way or another -- see
        // `TracingWaker`'s docs for why that means it never needs to forward wake-ups.
        // Unbounded: see `TracingWaker::sender`'s docs for why that's safe here despite the
        // repo's default of disallowing `mpsc::unbounded_channel`.
        #[allow(clippy::disallowed_methods)]
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let waker = TracingWaker::new_std_waker(sender);
        let mut traced_cx = Context::from_waker(&waker);
        match this.inner.poll(&mut traced_cx) {
            Poll::Ready(result) => Poll::Ready(Ok(result)),
            Poll::Pending => {
                let mut active_traces = Vec::new();
                while let Ok(TracedAwaitPoint { trace, alive }) = receiver.try_recv() {
                    if alive.upgrade().is_some() {
                        active_traces.push(trace);
                    }
                }
                Poll::Ready(Err(TimeoutElapsed { active_traces }))
            }
        }
    }
}

impl Display for TimeoutElapsed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.active_traces.is_empty() {
            f.write_str("timeout elapsed")?;
        } else {
            f.write_str("timeout elapsed at:\n")?;
            for (idx, trace) in self.active_traces.iter().enumerate() {
                writeln!(f, "trace {idx}:\n{trace}")?;
            }
        }
        Ok(())
    }
}

impl Error for TimeoutElapsed {}

/// Drive `fut` to completion, limiting its run time to `duration`. If `fut` doesn't finish in
/// time, returns [`TimeoutElapsed`] with a trace of every await point still pending at that
/// moment.
pub fn timeout<Fut>(duration: Duration, fut: Fut) -> TracedTimeout<Fut> {
    TracedTimeout {
        deadline: tokio::time::sleep(duration),
        inner: fut,
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::task::Poll;
    use std::task::Waker;
    use std::time::Duration;

    use tokio::sync::mpsc;
    use tracing::instrument;
    use tracing_error::ErrorLayer;
    use tracing_subscriber::layer::SubscriberExt;

    use super::TracingWaker;
    use super::timeout;

    /// Runs `fut` with a `tracing_error::ErrorLayer` installed, so [`SpanTrace::capture`] (used
    /// internally by [`timeout`]) picks up the `#[instrument]`ed spans below. Relies on
    /// `#[tokio::test]`'s default current-thread runtime so the thread-local default subscriber
    /// stays in effect across every poll of `fut`.
    async fn with_error_layer<Fut: Future>(fut: Fut) -> Fut::Output {
        let subscriber = tracing_subscriber::registry().with(ErrorLayer::default());
        let _guard = tracing::subscriber::set_default(subscriber);
        fut.await
    }

    /// Never completes and never touches its waker -- an await point with nothing registered to
    /// wake it.
    #[instrument]
    async fn pending_forever() {
        std::future::pending::<()>().await
    }

    /// Never completes; clones its waker into `slot` on every poll and keeps the clone alive, the
    /// way a future genuinely waiting on some external event would.
    #[instrument(skip(slot))]
    async fn awaits_retained_waker(slot: Arc<Mutex<Option<Waker>>>) {
        std::future::poll_fn(move |cx| {
            *slot.lock().unwrap() = Some(cx.waker().clone());
            Poll::<()>::Pending
        })
        .await
    }

    #[instrument(skip(slot))]
    async fn branch_a(slot: Arc<Mutex<Option<Waker>>>) {
        awaits_retained_waker(slot).await
    }

    #[instrument(skip(slot))]
    async fn branch_b(slot: Arc<Mutex<Option<Waker>>>) {
        awaits_retained_waker(slot).await
    }

    /// Never completes; clones its waker on every poll but immediately drops the clone, the way a
    /// future that merely inspects (rather than retains) its waker would.
    #[instrument]
    async fn drops_waker_immediately() {
        std::future::poll_fn(|cx| {
            let _ = cx.waker().clone();
            Poll::<()>::Pending
        })
        .await
    }

    /// Never completes; on every poll, clones its waker once and immediately drops that clone
    /// (touched but not retained), then clones it again and genuinely retains the second clone --
    /// covering a future whose single poll call touches the waker more than once, only one of
    /// which is a real await-point registration.
    #[instrument(skip(slot))]
    async fn drops_then_retains_waker(slot: Arc<Mutex<Option<Waker>>>) {
        std::future::poll_fn(move |cx| {
            let _ = cx.waker().clone();
            *slot.lock().unwrap() = Some(cx.waker().clone());
            Poll::<()>::Pending
        })
        .await
    }

    #[tokio::test]
    async fn completes_before_deadline() {
        let result = timeout(Duration::from_secs(10), async { 42 }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn elapses_with_no_pending_trace() {
        let err = with_error_layer(timeout(Duration::from_millis(10), pending_forever()))
            .await
            .unwrap_err();

        assert!(err.active_traces.is_empty());
        assert_eq!(err.to_string(), "timeout elapsed");
    }

    #[tokio::test]
    async fn elapses_with_one_pending_trace() {
        let slot = Arc::new(Mutex::new(None));
        let err = with_error_layer(timeout(
            Duration::from_millis(10),
            awaits_retained_waker(slot),
        ))
        .await
        .unwrap_err();

        assert_eq!(err.active_traces.len(), 1);
        assert!(err.to_string().contains("awaits_retained_waker"));
    }

    #[tokio::test]
    async fn elapses_with_multiple_pending_traces() {
        let err = with_error_layer(timeout(
            Duration::from_millis(10),
            futures::future::join(
                branch_a(Arc::new(Mutex::new(None))),
                branch_b(Arc::new(Mutex::new(None))),
            ),
        ))
        .await
        .unwrap_err();

        assert_eq!(err.active_traces.len(), 2);
        let rendered = err.to_string();
        assert!(rendered.contains("branch_a"));
        assert!(rendered.contains("branch_b"));
    }

    #[tokio::test]
    async fn dropped_waker_leaves_no_trace() {
        let err = with_error_layer(timeout(
            Duration::from_millis(10),
            drops_waker_immediately(),
        ))
        .await
        .unwrap_err();

        assert!(
            err.active_traces.is_empty(),
            "a waker clone dropped before the timeout's final poll returns shouldn't leave a \
             trace behind"
        );
    }

    #[tokio::test]
    async fn only_retained_clone_leaves_a_trace() {
        let slot = Arc::new(Mutex::new(None));
        let err = with_error_layer(timeout(
            Duration::from_millis(10),
            drops_then_retains_waker(slot),
        ))
        .await
        .unwrap_err();

        assert_eq!(
            err.active_traces.len(),
            1,
            "the touched-but-dropped clone shouldn't leave a trace; only the retained one should"
        );
    }

    /// Regression test: `wake()` must drop its boxed waker just like `wake_by_ref()` + `drop()`
    /// or `drop()` alone do, per the `RawWakerVTable` contract. Before the fix, `raw_wake` only
    /// borrowed the box, leaking it forever.
    #[test]
    fn wake_drops_the_waker_like_drop_does() {
        #[allow(clippy::disallowed_methods)]
        let (sender, receiver) = mpsc::unbounded_channel();
        let waker = TracingWaker::new_std_waker(sender);
        assert_eq!(receiver.sender_strong_count(), 1, "waker's own box");

        // Simulate a future registering for a wakeup elsewhere (e.g. a DB driver's completion
        // slot) by cloning the waker.
        let cloned = waker.clone();
        assert_eq!(receiver.sender_strong_count(), 2);

        // ...and later consuming it via `wake()`, as one-shot completion notifications do.
        cloned.wake();
        assert_eq!(
            receiver.sender_strong_count(),
            1,
            "wake() must free its box, not just notify"
        );

        drop(waker);
        assert_eq!(receiver.sender_strong_count(), 0);
    }

    /// `wake_by_ref` must not consume or free the box (unlike `wake()` above); a later explicit
    /// `drop` still must.
    #[test]
    fn wake_by_ref_then_drop_frees_the_box() {
        #[allow(clippy::disallowed_methods)]
        let (sender, receiver) = mpsc::unbounded_channel();
        let waker = TracingWaker::new_std_waker(sender);
        let cloned = waker.clone();
        assert_eq!(receiver.sender_strong_count(), 2);

        cloned.wake_by_ref();
        assert_eq!(
            receiver.sender_strong_count(),
            2,
            "wake_by_ref must not free the box"
        );

        drop(cloned);
        assert_eq!(receiver.sender_strong_count(), 1);
        drop(waker);
        assert_eq!(receiver.sender_strong_count(), 0);
    }

    /// Regression test: cloning the waker after its receiver has already been dropped (e.g.
    /// because `TracedTimeout::poll` already drained and returned) must not panic -- the send is
    /// best-effort.
    #[test]
    fn clone_after_receiver_dropped_does_not_panic() {
        #[allow(clippy::disallowed_methods)]
        let (sender, receiver) = mpsc::unbounded_channel();
        drop(receiver);

        let waker = TracingWaker::new_std_waker(sender);
        let cloned = waker.clone();
        drop(cloned);
        drop(waker);
    }
}
