// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;

use tokio::sync::watch;

/// A value that only moves forward, with waiters that resolve once it reaches a target.
///
/// `advance_to` is the only mutator and ignores anything at or below the current value, so
/// writers that observe progress out of order cannot move the mark backwards.
#[derive(Debug)]
pub struct HighWaterMark<T> {
    sender: watch::Sender<T>,
}

impl<T: Copy + Ord + Send + Sync + 'static> HighWaterMark<T> {
    pub fn new(initial: T) -> Self {
        Self {
            sender: watch::Sender::new(initial),
        }
    }

    pub fn current(&self) -> T {
        *self.sender.borrow()
    }

    /// Returns true if the mark moved.
    pub fn advance_to(&self, value: T) -> bool {
        self.sender.send_if_modified(|current| {
            if value > *current {
                *current = value;
                true
            } else {
                false
            }
        })
    }

    /// Resolves to the first observed value at or above `target`, or to `None` if the mark is
    /// dropped before reaching it. The returned future does not borrow `self`.
    pub fn wait_for(&self, target: T) -> impl Future<Output = Option<T>> + Send + 'static {
        let mut receiver = self.sender.subscribe();
        async move {
            receiver
                .wait_for(|current| *current >= target)
                .await
                .ok()
                .map(|current| *current)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn advance_only_moves_forward() {
        let mark = HighWaterMark::new(0u64);
        assert!(mark.advance_to(2));
        assert!(!mark.advance_to(1));
        assert!(!mark.advance_to(2));
        assert_eq!(mark.current(), 2);
    }

    #[tokio::test]
    async fn wait_for_already_reached() {
        let mark = HighWaterMark::new(5u64);
        assert_eq!(mark.wait_for(3).await, Some(5));
    }

    #[tokio::test]
    async fn wait_for_resolves_on_advance() {
        let mark = HighWaterMark::new(0u64);
        let wait = mark.wait_for(2);
        mark.advance_to(1);
        mark.advance_to(3);
        assert_eq!(wait.await, Some(3));
    }

    #[tokio::test]
    async fn wait_for_none_when_dropped() {
        let mark = HighWaterMark::new(0u64);
        let wait = mark.wait_for(1);
        drop(mark);
        assert_eq!(wait.await, None);
    }
}
