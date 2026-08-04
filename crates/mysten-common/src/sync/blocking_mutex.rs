// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mutex acquisition for code that may block an execution thread.
//!
//! An execution thread that waits for a mutex must release its execution permit so the
//! lock holder or other dependent execution can make progress. Under msim, it must also
//! yield its blocking-thread quantum instead of parking the OS thread, which would hang
//! the simulator.

use std::sync::Arc;

use parking_lot::{ArcMutexGuard, Mutex, RawMutex};

use crate::sync::execution_permit::release_execution_permit;

/// Acquires an owned mutex guard, releasing the current execution permit if acquisition
/// would block.
///
/// Must not be called from an async context. Under msim, contended acquisition may only
/// run on a blocking-pool thread, where it yields the thread's quantum between attempts.
pub fn lock<T: ?Sized>(mutex: Arc<Mutex<T>>) -> ArcMutexGuard<RawMutex, T> {
    // Fast path: never release the execution permit if the mutex is immediately available.
    if let Some(guard) = mutex.clone().try_lock_arc() {
        return guard;
    }

    release_execution_permit();

    #[cfg(msim)]
    loop {
        msim::task::yield_blocking();
        if let Some(guard) = mutex.clone().try_lock_arc() {
            return guard;
        }
    }

    #[cfg(not(msim))]
    mutex.lock_arc()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::execution_permit::set_execution_permit;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    struct DropFlag(Arc<AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn keeps_permit_when_uncontended() {
        let mutex = Arc::new(Mutex::new(()));
        let released = Arc::new(AtomicBool::new(false));
        let _permit = set_execution_permit(Box::new(DropFlag(released.clone())));

        let _guard = lock(mutex);

        assert!(
            !released.load(Ordering::SeqCst),
            "permit must be kept when the mutex is immediately available"
        );
    }

    #[cfg(not(msim))]
    #[test]
    fn releases_permit_when_contended() {
        let mutex = Arc::new(Mutex::new(()));
        let holder = mutex.clone().lock_arc();
        let released = Arc::new(AtomicBool::new(false));
        let released_waiter = released.clone();

        let waiter = std::thread::spawn(move || {
            let _permit = set_execution_permit(Box::new(DropFlag(released_waiter)));
            drop(lock(mutex));
        });

        while !released.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(1));
        }
        drop(holder);
        waiter.join().unwrap();
    }

    #[cfg(msim)]
    #[msim::sim_test]
    async fn contended_lock_releases_permit_and_yields() {
        // The waiter can only let this async task tell the holder to unlock if contended
        // acquisition drops its execution permit and yields its msim blocking-thread quantum.
        let mutex = Arc::new(Mutex::new(()));
        let holder_acquired = Arc::new(AtomicBool::new(false));
        let release_holder = Arc::new(AtomicBool::new(false));

        let holder = tokio::task::spawn_blocking({
            let mutex = mutex.clone();
            let holder_acquired = holder_acquired.clone();
            let release_holder = release_holder.clone();
            move || {
                let guard = mutex.lock();
                holder_acquired.store(true, Ordering::SeqCst);
                while !release_holder.load(Ordering::SeqCst) {
                    msim::task::yield_blocking();
                }
                drop(guard);
            }
        });

        while !holder_acquired.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        let released = Arc::new(AtomicBool::new(false));
        let waiter = tokio::task::spawn_blocking({
            let released = released.clone();
            move || {
                let _permit = set_execution_permit(Box::new(DropFlag(released)));
                drop(lock(mutex));
            }
        });

        while !released.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        release_holder.store(true, Ordering::SeqCst);

        holder.await.unwrap();
        waiter.await.unwrap();
    }
}
