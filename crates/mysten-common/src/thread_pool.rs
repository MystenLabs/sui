// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A dedicated pool of OS threads for long-running blocking work.
//!
//! Unlike `tokio::task::spawn_blocking`, which shares one process-global pool with every
//! other subsystem, a `DedicatedThreadPool` gives its owner a private, fixed-size set of
//! threads. This matters for work that may *park* a thread for a long time (e.g.
//! transaction execution blocking on a value another execution will produce): parked
//! threads neither starve unrelated `spawn_blocking` users nor compete with them for
//! capacity, and the owner can size the pool from its own admission policy.
//!
//! Jobs are fire-and-forget: completion is signaled by the job itself (or by guards it
//! drops). A panic in a job aborts the process - jobs are not expected to fail, and
//! silently swallowing a panic would let the caller conclude the work finished.
//!
//! Under msim, jobs are forwarded to the simulator's deterministic blocking pool via
//! `tokio::task::spawn_blocking` (the sim serializes threads and dequeues jobs in
//! seeded-random order, so a private pool would add nothing to isolation and would
//! break determinism). The sim pool's capacity is controlled by `MSIM_BLOCKING_THREADS`
//! (default 32), so `num_threads` is ignored there; sim callers must keep their
//! concurrent parked-job count below that capacity.

use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex, mpsc};

type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct DedicatedThreadPool {
    #[cfg(not(msim))]
    sender: Option<mpsc::Sender<Job>>,
    #[cfg(not(msim))]
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl DedicatedThreadPool {
    /// Creates a pool of `num_threads` OS threads named `{name}-{i}`.
    #[allow(unused_variables)]
    pub fn new(name: &str, num_threads: usize) -> Self {
        #[cfg(not(msim))]
        {
            assert!(num_threads > 0, "thread pool must have at least one thread");
            let (sender, receiver) = mpsc::channel::<Job>();
            let receiver = Arc::new(Mutex::new(receiver));
            let threads = (0..num_threads)
                .map(|i| {
                    let receiver = receiver.clone();
                    std::thread::Builder::new()
                        .name(format!("{name}-{i}"))
                        .spawn(move || worker_loop(receiver))
                        .expect("failed to spawn pool thread")
                })
                .collect();
            Self {
                sender: Some(sender),
                threads,
            }
        }
        #[cfg(msim)]
        Self {}
    }

    /// Enqueues a job. Jobs are picked up by idle threads; if all threads are busy the
    /// job waits in an unbounded queue. Callers that must not queue are responsible for
    /// admitting no more concurrent jobs than the pool has threads.
    pub fn spawn(&self, f: impl FnOnce() + Send + 'static) {
        #[cfg(not(msim))]
        self.sender
            .as_ref()
            .expect("sender present until drop")
            .send(Box::new(f))
            .expect("pool threads outlive the sender");

        // The sim's blocking pool wraps each job and delivers its panic through the
        // JoinHandle, so await it from a task to re-raise instead of swallowing it.
        // (A cancelled handle means the node was killed - not a job failure.)
        #[cfg(msim)]
        {
            let handle = tokio::task::spawn_blocking(f);
            tokio::task::spawn(async move {
                if let Err(e) = handle.await
                    && e.is_panic()
                {
                    std::panic::resume_unwind(e.into_panic());
                }
            });
        }
    }
}

#[cfg(not(msim))]
fn worker_loop(receiver: Arc<Mutex<mpsc::Receiver<Job>>>) {
    loop {
        // Holding the lock while parked in recv() is the standard shared-receiver
        // pattern: it serializes dequeueing, not job execution.
        let job = match receiver.lock().unwrap().recv() {
            Ok(job) => job,
            // Sender dropped and queue drained: pool is shutting down.
            Err(mpsc::RecvError) => return,
        };
        if std::panic::catch_unwind(AssertUnwindSafe(job)).is_err() {
            // The default panic hook has already printed the payload and backtrace.
            eprintln!("panic in dedicated thread pool job; aborting");
            std::process::abort();
        }
    }
}

#[cfg(not(msim))]
impl Drop for DedicatedThreadPool {
    fn drop(&mut self) {
        // Closing the channel lets workers drain remaining jobs and exit. Joining
        // requires that all jobs terminate, which pool owners must guarantee anyway.
        self.sender.take();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

#[cfg(all(test, not(msim)))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn runs_all_jobs_and_joins_on_drop() {
        let pool = DedicatedThreadPool::new("test-pool", 4);
        let count = Arc::new(AtomicUsize::new(0));
        for _ in 0..100 {
            let count = count.clone();
            pool.spawn(move || {
                count.fetch_add(1, Ordering::SeqCst);
            });
        }
        drop(pool);
        assert_eq!(count.load(Ordering::SeqCst), 100);
    }

    #[test]
    fn parked_jobs_do_not_block_other_threads() {
        let pool = DedicatedThreadPool::new("test-pool", 2);
        let (tx, rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        // Park one thread until released.
        pool.spawn(move || {
            rx.recv().unwrap();
        });
        // The other thread remains available.
        pool.spawn(move || {
            done_tx.send(()).unwrap();
        });
        done_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("second thread should run while first is parked");
        tx.send(()).unwrap();
    }
}
