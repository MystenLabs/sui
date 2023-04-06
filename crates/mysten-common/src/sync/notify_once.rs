// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::futures::Notified;
use tokio::sync::Notify;

/// Notify once allows waiter to register for certain conditions and unblocks waiter
/// when condition is signalled with `notify` method.
///
/// The functionality is somewhat similar to a tokio watch channel with subscribe method,
/// however it is much less error prone to use NotifyOnce rather then tokio watch.
///
/// Specifically with tokio watch you may miss notification,
/// if you subscribe to it after the value was changed
/// (Note that this is not a bug in tokio watch, but rather a mis-use of it).
///
/// NotifyOnce guarantees that wait() will return once notify() is called,
/// regardless of whether wait() was called before or after notify().
#[derive(Debug)]
pub struct NotifyOnce {
    notify: Mutex<Option<Arc<Notify>>>,
}

impl NotifyOnce {
    pub fn new() -> Self {
        Self::default()
    }

    /// Notify all waiters, present and future about event
    ///
    /// After this method all pending and future calls to .wait() will return
    ///
    /// This method returns errors if called more then once
    #[allow(clippy::result_unit_err)]
    pub fn notify(&self) -> Result<(), ()> {
        let Some(notify) = self.notify.lock().take() else { return Err(()) };
        // At this point all `register` either registered with current notify,
        // or will be returning immediately
        notify.notify_waiters();
        Ok(())
    }

    /// Awaits for `notify` method to be called.
    ///
    /// This future is cancellation safe.
    pub async fn wait(&self) {
        // Note that we only hold lock briefly when registering for notification
        // There is a bit of a trickery here with lock - we take a lock and if it is not empty,
        // we register .notified() first and then release lock
        //
        // This is to make sure no notification is lost because Notify::notify_waiters do not
        // notify waiters that register **after** notify_waiters was called
        let mut notify = None;
        let notified = self.make_notified(&mut notify);

        if let Some(notified) = notified {
            notified.await;
        }
    }

    // This made into separate function as it is only way to make compiler
    // not to hold `lock` in a generated async future.
    fn make_notified<'a>(&self, notify: &'a mut Option<Arc<Notify>>) -> Option<Notified<'a>> {
        let lock = self.notify.lock();
        *notify = lock.as_ref().cloned();
        notify.as_ref().map(|n| n.notified())
    }
}

impl Default for NotifyOnce {
    fn default() -> Self {
        let notify = Arc::new(Notify::new());
        let notify = Mutex::new(Some(notify));
        Self { notify }
    }
}

#[tokio::test]
async fn notify_once_test() {
    let notify_once = NotifyOnce::new();
    // Before notify() is called .wait() is not ready
    assert!(futures::future::poll_immediate(notify_once.wait())
        .await
        .is_none());
    let wait = notify_once.wait();
    notify_once.notify().unwrap();
    // Pending wait() call is ready now
    assert!(futures::future::poll_immediate(wait).await.is_some());
    // Take wait future and don't resolve it.
    // This makes sure lock is dropped properly and wait futures resolve independently of each other
    let _dangle_wait = notify_once.wait();
    // Any new wait() is immediately ready
    assert!(futures::future::poll_immediate(notify_once.wait())
        .await
        .is_some());
}
