use futures::ready;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{
    collections::VecDeque,
    future::poll_fn,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::sync::{
    mpsc::{self, error::SendError},
    watch,
};
use tokio_util::sync::{PollSendError, PollSender, ReusableBoxFuture};

use mysten_metrics::monitored_mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::ExExNotifications;
use crate::{event::ExExEvent, head::FinishedExExHeight, notification::ExExNotification};

/// The execution extension manager.
///
/// The manager is responsible for:
///
/// - Receiving relevant events from the rest of the node, and sending these to the execution
///   extensions
/// - Backpressure
/// - Error handling
/// - Monitoring
#[derive(Debug)]
pub struct ExExManager {
    /// Handles to communicate with the `ExEx`'s.
    pub exex_handles: Vec<ExExHandle>,

    /// [`ExExNotification`] channel from the [`ExExManagerHandle`]s.
    pub handle_rx: UnboundedReceiver<ExExNotification>,

    /// The minimum notification ID currently present in the buffer.
    min_id: usize,
    /// Monotonically increasing ID for [`ExExNotification`]s.
    next_id: usize,

    /// Internal buffer of [`ExExNotification`]s.
    ///
    /// The first element of the tuple is a monotonically increasing ID unique to the notification
    /// (the second element of the tuple).
    buffer: VecDeque<(usize, ExExNotification)>,
    /// Max size of the internal state notifications buffer.
    max_capacity: usize,
    /// Current state notifications buffer capacity.
    ///
    /// Used to inform the execution stage of possible batch sizes.
    current_capacity: Arc<AtomicUsize>,

    /// Whether the manager is ready to receive new notifications.
    is_ready: watch::Sender<bool>,

    /// The finished height of all `ExEx`'s.
    finished_height: watch::Sender<FinishedExExHeight>,

    /// A handle to the `ExEx` manager.
    handle: ExExManagerHandle,
}

impl ExExManager {
    /// Create a new [`ExExManager`].
    ///
    /// You must provide an [`ExExHandle`] for each `ExEx` and the maximum capacity of the
    /// notification buffer in the manager.
    ///
    /// When the capacity is exceeded (which can happen if an `ExEx` is slow) no one can send
    /// notifications over [`ExExManagerHandle`]s until there is capacity again.
    pub fn new(handles: Vec<ExExHandle>, max_capacity: usize) -> Self {
        let num_exexs = handles.len();

        let (handle_tx, handle_rx) = unbounded_channel("exexes");
        let (is_ready_tx, is_ready_rx) = watch::channel(true);
        let (finished_height_tx, finished_height_rx) = watch::channel(if num_exexs == 0 {
            FinishedExExHeight::NoExExs
        } else {
            FinishedExExHeight::NotReady
        });

        let current_capacity = Arc::new(AtomicUsize::new(max_capacity));

        Self {
            exex_handles: handles,

            handle_rx,

            min_id: 0,
            next_id: 0,
            buffer: VecDeque::with_capacity(max_capacity),
            max_capacity,
            current_capacity: Arc::clone(&current_capacity),

            is_ready: is_ready_tx,
            finished_height: finished_height_tx,

            handle: ExExManagerHandle {
                exex_tx: handle_tx,
                num_exexs,
                is_ready_receiver: is_ready_rx.clone(),
                is_ready: ReusableBoxFuture::new(make_wait_future(is_ready_rx)),
                current_capacity,
                finished_height: finished_height_rx,
            },
        }
    }

    /// Returns the handle to the manager.
    pub fn handle(&self) -> ExExManagerHandle {
        self.handle.clone()
    }

    /// Updates the current buffer capacity and notifies all `is_ready` watchers of the manager's
    /// readiness to receive notifications.
    fn update_capacity(&self) {
        let capacity = self.max_capacity.saturating_sub(self.buffer.len());
        self.current_capacity.store(capacity, Ordering::Relaxed);

        // we can safely ignore if the channel is closed, since the manager always holds it open
        // internally
        let _ = self.is_ready.send(capacity > 0);
    }

    /// Pushes a new notification into the managers internal buffer, assigning the notification a
    /// unique ID.
    pub fn push_notification(&mut self, notification: ExExNotification) {
        let next_id = self.next_id;
        self.buffer.push_back((next_id, notification));
        self.next_id += 1;
    }
}

impl std::future::Future for ExExManager {
    type Output = anyhow::Result<()>;

    /// Main loop of the [`ExExManager`]. The order of operations is as follows:
    /// 1. Handle incoming ExEx events.
    /// 2. Drain [`ExExManagerHandle`] notifications, push them to the internal buffer and update
    ///    the internal buffer capacity.
    /// 3. Send notifications from the internal buffer to those ExExes that are ready to receive new
    ///    notifications.
    /// 4. Remove notifications from the internal buffer that have been sent to **all** ExExes and
    ///    update the internal buffer capacity.
    /// 5. Update the channel with the lowest [`FinishedExExHeight`] among all ExExes.
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        // Handle incoming ExEx events
        for exex in &mut this.exex_handles {
            while let Poll::Ready(Some(event)) = exex.receiver.poll_recv(cx) {
                match event {
                    ExExEvent::FinishedHeight(height) => exex.finished_height = Some(height),
                }
            }
        }

        // Drain handle notifications
        while this.buffer.len() < this.max_capacity {
            if let Poll::Ready(Some(notification)) = this.handle_rx.poll_recv(cx) {
                this.push_notification(notification);
                continue;
            }
            break;
        }

        // Update capacity
        this.update_capacity();

        // Advance all poll senders
        let mut min_id = usize::MAX;
        for idx in (0..this.exex_handles.len()).rev() {
            let mut exex = this.exex_handles.swap_remove(idx);

            // It is a logic error for this to ever underflow since the manager manages the
            // notification IDs
            let notification_index = exex
                .next_notification_id
                .checked_sub(this.min_id)
                .expect("exex expected notification ID outside the manager's range");
            if let Some(notification) = this.buffer.get(notification_index) {
                if let Poll::Ready(Err(err)) = exex.send(cx, notification) {
                    // The channel was closed, which is irrecoverable for the manager
                    return Poll::Ready(Err(err.into()));
                }
            }
            min_id = min_id.min(exex.next_notification_id);
            this.exex_handles.push(exex);
        }

        // Remove processed buffered notifications
        this.buffer.retain(|&(id, _)| id >= min_id);
        this.min_id = min_id;

        // Update capacity
        this.update_capacity();

        // Update watch channel block number
        let finished_height = this
            .exex_handles
            .iter_mut()
            .try_fold(u64::MAX, |curr, exex| {
                exex.finished_height
                    .map_or(Err(()), |height| Ok(height.min(curr)))
            });

        if let Ok(finished_height) = finished_height {
            let _ = this
                .finished_height
                .send(FinishedExExHeight::Height(finished_height));
        }

        Poll::Pending
    }
}

/// A handle to communicate with the [`ExExManager`].
#[derive(Debug)]
pub struct ExExManagerHandle {
    /// Channel to send notifications to the `ExEx` manager.
    exex_tx: UnboundedSender<ExExNotification>,
    /// The number of `ExEx`'s running on the node.
    num_exexs: usize,
    /// A watch channel denoting whether the manager is ready for new notifications or not.
    /// This is stored internally alongside a `ReusableBoxFuture` representation of the same value.
    /// This field is only used to create a new `ReusableBoxFuture` when the handle is cloned,
    /// but is otherwise unused.
    is_ready_receiver: watch::Receiver<bool>,
    /// A reusable future that resolves when the manager is ready for new
    /// notifications.
    is_ready: ReusableBoxFuture<'static, watch::Receiver<bool>>,
    /// The current capacity of the manager's internal notification buffer.
    current_capacity: Arc<AtomicUsize>,
    /// The finished height of all `ExEx`'s.
    finished_height: watch::Receiver<FinishedExExHeight>,
}

impl ExExManagerHandle {
    /// Synchronously send a notification over the channel to all execution extensions.
    ///
    /// Senders should call [`Self::has_capacity`] first.
    pub fn send(&self, notification: ExExNotification) -> Result<(), SendError<ExExNotification>> {
        self.exex_tx.send(notification)
    }

    /// Asynchronously send a notification over the channel to all execution extensions.
    ///
    /// The returned future resolves when the notification has been delivered. If there is no
    /// capacity in the channel, the future will wait.
    pub async fn send_async(
        &mut self,
        notification: ExExNotification,
    ) -> Result<(), SendError<ExExNotification>> {
        self.ready().await;
        self.exex_tx.send(notification)
    }

    /// Get the current capacity of the `ExEx` manager's internal notification buffer.
    pub fn capacity(&self) -> usize {
        self.current_capacity.load(Ordering::Relaxed)
    }

    /// Whether there is capacity in the `ExEx` manager's internal notification buffer.
    ///
    /// If this returns `false`, the owner of the handle should **NOT** send new notifications over
    /// the channel until the manager is ready again, as this can lead to unbounded memory growth.
    pub fn has_capacity(&self) -> bool {
        self.capacity() > 0
    }

    /// Returns `true` if there are `ExEx`'s installed in the node.
    pub const fn has_exexs(&self) -> bool {
        self.num_exexs > 0
    }

    /// The finished height of all `ExEx`'s.
    pub fn finished_height(&self) -> watch::Receiver<FinishedExExHeight> {
        self.finished_height.clone()
    }

    /// Wait until the manager is ready for new notifications.
    pub async fn ready(&mut self) {
        poll_fn(|cx| self.poll_ready(cx)).await
    }

    /// Wait until the manager is ready for new notifications.
    pub fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        let rx = ready!(self.is_ready.poll(cx));
        self.is_ready.set(make_wait_future(rx));
        Poll::Ready(())
    }
}

impl Clone for ExExManagerHandle {
    fn clone(&self) -> Self {
        Self {
            exex_tx: self.exex_tx.clone(),
            num_exexs: self.num_exexs,
            is_ready_receiver: self.is_ready_receiver.clone(),
            is_ready: ReusableBoxFuture::new(make_wait_future(self.is_ready_receiver.clone())),
            current_capacity: self.current_capacity.clone(),
            finished_height: self.finished_height.clone(),
        }
    }
}

/// A handle to an `ExEx` used by the [`ExExManager`] to communicate with `ExEx`'s.
///
/// A handle should be created for each `ExEx` with a unique ID. The channels returned by
/// [`ExExHandle::new`] should be given to the `ExEx`, while the handle itself should be given to
/// the manager in [`ExExManager::new`].
#[derive(Debug)]
pub struct ExExHandle {
    /// The execution extension's ID.
    pub id: String,
    /// Channel to send [`ExExNotification`]s to the `ExEx`.
    pub sender: PollSender<ExExNotification>,
    /// Channel to receive [`ExExEvent`]s from the `ExEx`.
    receiver: UnboundedReceiver<ExExEvent>,
    /// The ID of the next notification to send to this `ExEx`.
    next_notification_id: usize,
    /// The finished block of the `ExEx`.
    ///
    /// If this is `None`, the `ExEx` has not emitted a `FinishedHeight` event.
    finished_height: Option<CheckpointSequenceNumber>,
}

impl ExExHandle {
    pub fn new(id: String) -> (Self, UnboundedSender<ExExEvent>, ExExNotifications) {
        let (notification_tx, notification_rx) = mpsc::channel(1);
        let (event_tx, event_rx) = unbounded_channel("exex_channel");
        let notifications = ExExNotifications::new(notification_rx);

        (
            Self {
                id: id.clone(),
                sender: PollSender::new(notification_tx),
                receiver: event_rx,
                next_notification_id: 0,
                finished_height: None,
            },
            event_tx,
            notifications,
        )
    }

    /// Reserves a slot in the `PollSender` channel and sends the notification if the slot was
    /// successfully reserved.
    ///
    /// When the notification is sent, it is considered delivered.
    fn send(
        &mut self,
        cx: &mut Context<'_>,
        (notification_id, notification): &(usize, ExExNotification),
    ) -> Poll<Result<(), PollSendError<ExExNotification>>> {
        match self.sender.poll_reserve(cx) {
            Poll::Ready(Ok(())) => (),
            other => return other,
        }

        match self.sender.send_item(notification.clone()) {
            Ok(()) => {
                self.next_notification_id = notification_id + 1;
                Poll::Ready(Ok(()))
            }
            Err(err) => Poll::Ready(Err(err)),
        }
    }
}

/// Creates a future that resolves once the given watch channel receiver is true.
async fn make_wait_future(mut rx: watch::Receiver<bool>) -> watch::Receiver<bool> {
    let _ = rx.wait_for(|ready| *ready).await;
    rx
}
