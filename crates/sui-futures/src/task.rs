// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use tokio::task::{JoinError, JoinHandle};

/// A wrapper around `JoinHandle` that aborts the task when dropped.
///
/// The abort on drop does not wait for the task to finish, it simply sends the abort signal.
#[must_use = "Dropping the handle aborts the task immediately"]
#[derive(Debug)]
pub struct TaskGuard<T>(JoinHandle<T>);

impl<T> TaskGuard<T> {
    pub fn new(handle: JoinHandle<T>) -> Self {
        Self(handle)
    }
}

impl<T> Future for TaskGuard<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}

impl<T> AsRef<JoinHandle<T>> for TaskGuard<T> {
    fn as_ref(&self) -> &JoinHandle<T> {
        &self.0
    }
}

impl<T> Drop for TaskGuard<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::oneshot;

    use super::*;

    #[tokio::test]
    async fn test_abort_on_drop() {
        let (mut tx, rx) = oneshot::channel::<()>();

        let guard = TaskGuard::new(tokio::spawn(async move {
            let _ = rx.await;
        }));

        // When the guard is dropped, the task should be aborted, cleaning up its future, which
        // will close the receiving side of the channel.
        drop(guard);
        tokio::time::timeout(Duration::from_millis(100), tx.closed())
            .await
            .unwrap();
    }
}
