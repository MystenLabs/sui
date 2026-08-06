// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fmt::Display;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Weak;
use std::task::Context;
use std::task::Poll;
use std::task::RawWaker;
use std::task::RawWakerVTable;
use std::task::Waker;
use std::time::Duration;

use pin_project::pin_project;
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
    pub active_traces: Vec<Arc<SpanTrace>>,
}

/// Shared state for a single timeout's final poll: the traces captured so far, and the real
/// waker to forward wake-ups to. Each clone's trace is only present in `active_traces` for as
/// long as that clone is alive -- once it's dropped, its `Weak` naturally stops upgrading, so
/// nothing needs to be nulled out by hand.
struct TracingWakerInner {
    active_traces: Mutex<Vec<Weak<SpanTrace>>>,
    inner_waker: Waker,
}

/// A `Waker` that captures a [`SpanTrace`] every time it's cloned -- each clone marks a distinct
/// still-pending await point -- and forwards actual wake-ups to `inner_waker`.
struct TracingWaker {
    inner: Arc<TracingWakerInner>,
    /// This clone's own trace, kept alive for as long as this waker is (never read directly --
    /// `active_traces` observes it through the `Weak` pushed alongside it). `None` for the
    /// original waker, which isn't itself an await point.
    _trace: Option<Arc<SpanTrace>>,
}

impl TracingWakerInner {
    fn new(inner_waker: Waker) -> Arc<Self> {
        Arc::new(Self {
            active_traces: Mutex::new(Vec::with_capacity(4)),
            inner_waker,
        })
    }

    fn take_traces(&self) -> Vec<Arc<SpanTrace>> {
        let traces = std::mem::take(&mut *self.active_traces.lock().unwrap());
        traces
            .into_iter()
            .filter_map(|trace| trace.upgrade())
            .collect()
    }
}

impl TracingWaker {
    const VTABLE: RawWakerVTable = RawWakerVTable::new(
        Self::raw_clone,
        Self::raw_wake,
        Self::raw_wake_by_ref,
        Self::raw_drop,
    );

    fn new_std_waker(inner: Arc<TracingWakerInner>) -> Waker {
        let data = Box::into_raw(Box::new(Self {
            inner,
            _trace: None,
        }));
        // SAFETY: `data` was just obtained from `Box::into_raw`, so it's a unique, valid pointer
        // to a `Box<Self>` -- the precondition every vtable function below relies on.
        unsafe { Waker::new(data as *const (), &Self::VTABLE) }
    }

    fn clone_capturing_trace(&self) -> Box<Self> {
        let trace = Arc::new(SpanTrace::capture());
        self.inner
            .active_traces
            .lock()
            .unwrap()
            .push(Arc::downgrade(&trace));
        Box::new(Self {
            inner: self.inner.clone(),
            _trace: Some(trace),
        })
    }

    unsafe fn raw_clone(data: *const ()) -> RawWaker {
        // SAFETY: see `new_std_waker`.
        let this = unsafe { &*(data as *const Self) };
        let cloned = this.clone_capturing_trace();
        RawWaker::new(Box::into_raw(cloned) as *const (), &Self::VTABLE)
    }

    unsafe fn raw_wake(data: *const ()) {
        // SAFETY: `wake` consumes the waker per the `RawWakerVTable` contract: forward the
        // wake-up via a shared reference first, then reclaim and drop the box. Both steps
        // independently satisfy the precondition in `new_std_waker`.
        unsafe { Self::raw_wake_by_ref(data) };
        unsafe { Self::raw_drop(data) };
    }

    unsafe fn raw_wake_by_ref(data: *const ()) {
        // SAFETY: see `new_std_waker`.
        let this = unsafe { &*(data as *const Self) };
        this.inner.inner_waker.wake_by_ref();
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
        // clones its waker.
        let inner = TracingWakerInner::new(cx.waker().clone());
        let waker = TracingWaker::new_std_waker(inner.clone());
        let mut traced_cx = Context::from_waker(&waker);
        match this.inner.poll(&mut traced_cx) {
            Poll::Ready(result) => Poll::Ready(Ok(result)),
            Poll::Pending => Poll::Ready(Err(TimeoutElapsed {
                active_traces: inner.take_traces(),
            })),
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

    use tracing::instrument;
    use tracing_error::ErrorLayer;
    use tracing_subscriber::layer::SubscriberExt;

    use super::TracingWaker;
    use super::TracingWakerInner;
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

    /// Regression test: `wake()` must drop its boxed waker just like `wake_by_ref()` + `drop()`
    /// or `drop()` alone do, per the `RawWakerVTable` contract. Before the fix, `raw_wake` only
    /// borrowed the box, leaking it (and the shared `Arc<TracingWakerInner>`) forever.
    #[test]
    fn wake_drops_the_waker_like_drop_does() {
        let inner = TracingWakerInner::new(Waker::noop().clone());
        let waker = TracingWaker::new_std_waker(inner.clone());
        assert_eq!(Arc::strong_count(&inner), 2, "local ref + waker's own box");

        // Simulate a future registering for a wakeup elsewhere (e.g. a DB driver's completion
        // slot) by cloning the waker.
        let cloned = waker.clone();
        assert_eq!(Arc::strong_count(&inner), 3);

        // ...and later consuming it via `wake()`, as one-shot completion notifications do.
        cloned.wake();
        assert_eq!(
            Arc::strong_count(&inner),
            2,
            "wake() must free its box, not just notify"
        );

        drop(waker);
        assert_eq!(Arc::strong_count(&inner), 1);
    }
}
