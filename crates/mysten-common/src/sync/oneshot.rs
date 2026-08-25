// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A oneshot channel whose receiver blocks the calling OS thread.
//!
//! This is the synchronous analogue of `tokio::sync::oneshot`, for code that must wait
//! for a value produced elsewhere without being async (e.g. execution threads blocking
//! until an object version is committed). It is a thin wrapper over the [`oneshot`]
//! crate that adds a [`Receiver::blocking_recv`] tailored to Sui's execution model:
//!
//! * The wait always begins with a non-blocking `try_recv`, so a value that is already
//!   available is returned without any side effects.
//! * If it must block, it first [releases the thread's execution permit] so other
//!   execution can proceed (see that module for the deadlock rationale).
//! * Under msim, parking the OS thread would hang the single-threaded simulator, so it
//!   instead polls in a loop, yielding the simulated thread quantum between checks. This
//!   would busy-wait on a real system, but the simulator only wakes the thread at a
//!   controlled rate, and only blocking-pool threads may wait this way.
//!
//! [releases the thread's execution permit]: crate::sync::execution_permit::release_execution_permit

use crate::sync::execution_permit::release_execution_permit;

/// Create a new oneshot channel.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let (sender, receiver) = oneshot::channel();
    (Sender(sender), Receiver(receiver))
}

pub struct Sender<T>(oneshot::Sender<T>);
pub struct Receiver<T>(oneshot::Receiver<T>);

/// Error returned by `blocking_recv` when the sender was dropped without sending.
#[derive(Debug, PartialEq, Eq)]
pub struct RecvError;

/// Error returned by `try_recv` when no value is available.
#[derive(Debug, PartialEq, Eq)]
pub enum TryRecvError {
    /// The sender has not sent a value yet.
    Empty,
    /// The sender was dropped without sending.
    Closed,
}

impl<T> Sender<T> {
    /// Send a value, consuming the sender. Returns the value back if the receiver was
    /// already dropped.
    pub fn send(self, value: T) -> Result<(), T> {
        self.0.send(value).map_err(|e| e.into_inner())
    }

    /// Whether the receiver has been dropped (i.e. a send would fail).
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }
}

impl<T> Receiver<T> {
    /// Wait until a value is sent (or the sender is dropped), blocking the calling
    /// thread.
    ///
    /// Must not be called from an async context. Under msim it may only be called from
    /// a blocking-pool thread (e.g. inside `spawn_blocking`); it yields the thread's
    /// quantum between readiness checks.
    pub fn blocking_recv(self) -> Result<T, RecvError> {
        // Fast path: never release the execution permit if a value is already available.
        match self.0.try_recv() {
            Ok(value) => return Ok(value),
            Err(oneshot::TryRecvError::Disconnected) => return Err(RecvError),
            Err(oneshot::TryRecvError::Empty) => {}
        }

        // We are about to block; give up our execution permit (if any) so that other
        // execution can make progress. Blocking while holding it risks deadlock under
        // limited execution concurrency.
        release_execution_permit();

        #[cfg(msim)]
        loop {
            match self.0.try_recv() {
                Ok(value) => return Ok(value),
                Err(oneshot::TryRecvError::Disconnected) => return Err(RecvError),
                Err(oneshot::TryRecvError::Empty) => msim::task::yield_blocking(),
            }
        }

        #[cfg(not(msim))]
        self.0.recv().map_err(|_| RecvError)
    }

    /// Return the value if one has been sent, without blocking.
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        match self.0.try_recv() {
            Ok(value) => Ok(value),
            Err(oneshot::TryRecvError::Empty) => Err(TryRecvError::Empty),
            Err(oneshot::TryRecvError::Disconnected) => Err(TryRecvError::Closed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::execution_permit::set_execution_permit;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    #[cfg(not(msim))]
    use std::time::Duration;

    struct DropFlag(Arc<AtomicBool>);
    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn send_then_recv() {
        let (tx, rx) = channel();
        tx.send(42).unwrap();
        assert_eq!(rx.blocking_recv(), Ok(42));
    }

    // These tests block a real OS thread and rely on it being unparked by another
    // thread. Under msim, `blocking_recv` yields via `msim::task::yield_blocking`, which
    // must run on a simulator blocking-pool thread rather than a raw `std::thread`, so
    // they only run outside the simulator. The msim blocking path is exercised by the
    // execution-layer simtests that use these primitives.
    #[cfg(not(msim))]
    #[test]
    fn recv_blocks_until_send() {
        let (tx, rx) = channel();
        let handle = std::thread::spawn(move || rx.blocking_recv());
        std::thread::sleep(Duration::from_millis(50));
        tx.send(7).unwrap();
        assert_eq!(handle.join().unwrap(), Ok(7));
    }

    #[cfg(not(msim))]
    #[test]
    fn sender_drop_unblocks_recv() {
        let (tx, rx) = channel::<u64>();
        let handle = std::thread::spawn(move || rx.blocking_recv());
        std::thread::sleep(Duration::from_millis(50));
        drop(tx);
        assert_eq!(handle.join().unwrap(), Err(RecvError));
    }

    #[test]
    fn receiver_drop_closes_sender() {
        let (tx, rx) = channel::<u64>();
        assert!(!tx.is_closed());
        drop(rx);
        assert!(tx.is_closed());
        assert_eq!(tx.send(1), Err(1));
    }

    #[test]
    fn try_recv() {
        let (tx, mut rx) = channel();
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        tx.send(5).unwrap();
        assert_eq!(rx.try_recv(), Ok(5));
    }

    #[test]
    fn keeps_permit_when_value_ready() {
        let (tx, rx) = channel::<u8>();
        tx.send(9).unwrap();
        let released = Arc::new(AtomicBool::new(false));
        let _guard = set_execution_permit(Box::new(DropFlag(released.clone())));
        assert_eq!(rx.blocking_recv(), Ok(9));
        assert!(
            !released.load(Ordering::SeqCst),
            "permit must be kept when the value is already available"
        );
    }

    #[cfg(not(msim))]
    #[test]
    fn releases_permit_when_blocking() {
        let (tx, rx) = channel::<u8>();
        let released = Arc::new(AtomicBool::new(false));
        let released_recv = released.clone();
        let handle = std::thread::spawn(move || {
            let _guard = set_execution_permit(Box::new(DropFlag(released_recv)));
            rx.blocking_recv()
        });
        // The receiver is now blocked; it should already have released its permit.
        std::thread::sleep(Duration::from_millis(50));
        assert!(
            released.load(Ordering::SeqCst),
            "permit must be released while blocked"
        );
        tx.send(3).unwrap();
        assert_eq!(handle.join().unwrap(), Ok(3));
    }
}
