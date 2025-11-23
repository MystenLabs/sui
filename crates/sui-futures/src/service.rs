// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::panic;
use std::time::Duration;

use futures::future;
use futures::future::BoxFuture;
use futures::future::FutureExt;
use tap::TapFallible;
use tokio::signal;
use tokio::task::JoinSet;
use tokio::time::timeout;
use tracing::error;
use tracing::info;

/// Default grace period for shutdown.
///
/// After shutdown signals are sent, tasks have this duration to complete gracefully before being
/// forcefully aborted.
pub const GRACE: Duration = Duration::from_secs(30);

/// A collection of related tasks that succeed or fail together, consisting of:
///
/// - A set of primary tasks, which control the lifetime of the service in the happy path. When all
///   primary tasks complete successfully or have been cancelled, the service will start a graceful
///   shutdown.
///
/// - A set of secondary tasks, which run alongside the primary tasks, but do not extend the
///   service's lifetime (The service will not wait for all secondary tasks to complete or be
///   cancelled before triggering a shutdown).
///
/// - A set of exit signals, which are executed when the service wants to trigger graceful
///   shutdown.
///
/// Any task (primary or secondary) failing by returning an error, or panicking, will also trigger
/// a graceful shutdown.
///
/// If graceful shutdown takes longer than the grace period, or another task fails during shutdown,
/// all remaining tasks are aborted and dropped immediately. Tasks are expected to clean-up after
/// themselves when dropped (e.g. if a task has spawned its own sub-tasks, these should also be
/// aborted when the parent task is dropped).
#[must_use = "Dropping the service aborts all its tasks immediately"]
#[derive(Default)]
pub struct Service {
    /// Futures that are run when the service is instructed to shutdown gracefully.
    exits: Vec<BoxFuture<'static, ()>>,

    /// Tasks that control the lifetime of the service in the happy path.
    fsts: JoinSet<anyhow::Result<()>>,

    /// Tasks that run alongside the primary tasks, but do not extend the service's lifetime.
    snds: JoinSet<anyhow::Result<()>>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Service has been terminated gracefully")]
    Terminated,

    #[error("Service has been aborted due to ungraceful shutdown")]
    Aborted,

    #[error(transparent)]
    Task(anyhow::Error),
}

impl Service {
    /// Create a new, empty service.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a primary task.
    ///
    /// The task will start running in the background immediately, once added. It is expected to
    /// clean up after itself when it is dropped, which will happen when it is aborted
    /// (non-graceful shutdown).
    pub fn spawn(
        mut self,
        task: impl Future<Output = anyhow::Result<()>> + Send + 'static,
    ) -> Self {
        self.fsts.spawn(task);
        self
    }

    /// Add a primary task that aborts immediately on graceful shutdown.
    ///
    /// This is useful for tasks that don't need a graceful shutdown.
    pub fn spawn_aborting(
        mut self,
        task: impl Future<Output = anyhow::Result<()>> + Send + 'static,
    ) -> Self {
        let h = self.fsts.spawn(task);
        self.with_shutdown_signal(async move { h.abort() })
    }

    /// Add a shutdown signal.
    ///
    /// This future will be executed when the service is instructed to shutdown gracefully, before
    /// a grace period timer starts (in which the task receiving the shutdown signal is expected to
    /// wind down and exit cleanly).
    ///
    /// Evaluation order of shutdown signals is non-determinate.
    pub fn with_shutdown_signal(mut self, exit: impl Future<Output = ()> + Send + 'static) -> Self {
        self.exits.push(exit.boxed());
        self
    }

    /// Add the tasks and signals from `other` into `self`.
    pub fn merge(mut self, mut other: Service) -> Self {
        self.exits.extend(other.exits);

        if !other.fsts.is_empty() {
            self.fsts.spawn(async move { run(&mut other.fsts).await });
        }

        if !other.snds.is_empty() {
            self.snds.spawn(async move { run(&mut other.snds).await });
        }

        self
    }

    /// Attach `other` to `self` as a secondary service.
    ///
    /// All its tasks (primary and secondary) will be treated as secondary tasks of `self`.
    pub fn attach(mut self, mut other: Service) -> Self {
        self.exits.extend(other.exits);

        if !other.fsts.is_empty() {
            self.snds.spawn(async move { run(&mut other.fsts).await });
        }

        if !other.snds.is_empty() {
            self.snds.spawn(async move { run(&mut other.snds).await });
        }

        self
    }

    /// Runs the service, waiting for interrupt signals from the operating system to trigger
    /// graceful shutdown, within the defualt grace period.
    pub async fn main(self) -> Result<(), Error> {
        self.wait_for_shutdown(GRACE, terminate).await
    }

    /// Waits for an exit condition to trigger shutdown, within `grace` period. Detects the
    /// following exit conditions:
    ///
    /// - All primary tasks complete successfully or are cancelled (returns `Ok(())`).
    /// - Any task (primary or secondary) fails or panics (returns `Err(Error::Task(_))`).
    /// - The `terminate` future completes (returns `Err(Error::Terminated)`).
    ///
    /// Any tasks that do not complete within the grace period are aborted. Aborted tasks are not
    /// joined, they are simply dropped (returns `Err(Error::Aborted)` regardless of the primary
    /// reason for shutting down).
    async fn wait_for_shutdown<T: Future<Output = ()>>(
        mut self,
        grace: Duration,
        mut terminate: impl FnMut() -> T,
    ) -> Result<(), Error> {
        let exec = tokio::select! {
            res = self.join() => {
                res.map_err(Error::Task)
            }

            _ = terminate() => {
                info!("Termination received");
                Err(Error::Terminated)
            }
        };

        info!("Shutting down gracefully...");
        tokio::select! {
            res = timeout(grace, self.shutdown()) => {
                match res {
                    Ok(Ok(())) => {},
                    Ok(Err(_)) => return Err(Error::Aborted),
                    Err(_) => {
                        error!("Grace period elapsed, aborting...");
                        return Err(Error::Aborted);
                    }
                }
            }

            _ = terminate() => {
                error!("Termination received during shutdown, aborting...");
                return Err(Error::Aborted);
            },
        }

        exec
    }

    /// Wait until all primary tasks in the service either complete successfully or are cancelled,
    /// or one task fails.
    ///
    /// This operation does not consume the service, so that it can be interacted with further in
    /// case of an error.
    pub async fn join(&mut self) -> anyhow::Result<()> {
        tokio::select! {
            res = run(&mut self.fsts) => {
                res.tap_err(|e| error!("Primary task failure: {e:#}"))
            },

            res = run_secondary(&mut self.snds) => {
                res.tap_err(|e| error!("Secondary task failure: {e:#}"))
            }
        }
    }

    /// Trigger a graceful shutdown of the service.
    ///
    /// Returns with an error if any of the constituent tasks produced an error during shutdown,
    /// otherwise waits for all tasks (primary and secondy) to complete successfully.
    pub async fn shutdown(mut self) -> Result<(), Error> {
        let _ = future::join_all(self.exits).await;
        if let Err(e) = future::try_join(run(&mut self.fsts), run(&mut self.snds)).await {
            error!("Task failure during shutdown: {e:#}");
            return Err(Error::Task(e));
        }

        Ok(())
    }
}

// SAFETY: `Service` is not `Send` by default because `self.exits` is not `Sync`, but it is only
// ever accessed through exclusive references (`&mut self` or `self`), so it cannot be accessed
// through multiple threads simultaneously.
unsafe impl Sync for Service {}

impl fmt::Debug for Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Service")
            .field("exits", &self.exits.len())
            .field("fsts", &self.fsts)
            .field("snds", &self.snds)
            .finish()
    }
}

/// Wait until all tasks in `tasks` complete successfully or is cancelled, or any individual task
/// fails or panics.
async fn run(tasks: &mut JoinSet<anyhow::Result<()>>) -> anyhow::Result<()> {
    while let Some(res) = tasks.join_next().await {
        match res {
            Ok(Ok(())) => continue,
            Ok(Err(e)) => return Err(e),

            Err(e) => {
                if e.is_panic() {
                    panic::resume_unwind(e.into_panic());
                }
            }
        }
    }

    Ok(())
}

/// Like `run` but never completes successfully (only propagates errors).
///
/// If the secondary tasks do all complete successfully, this future holds off indefinitely, to
/// give the primary tasks a chance to complete.
async fn run_secondary(tasks: &mut JoinSet<anyhow::Result<()>>) -> anyhow::Result<()> {
    run(tasks).await?;
    std::future::pending().await
}

/// Waits for various termination signals from the operating system.
///
/// On unix systems, this waits for either `SIGINT` or `SIGTERM`, while on other systems it will
/// only wait for `SIGINT`.
pub async fn terminate() {
    tokio::select! {
        _ = signal::ctrl_c() => {},

        _ = async {
            #[cfg(unix)]
            let _ = signal::unix::signal(signal::unix::SignalKind::terminate()).unwrap().recv().await;

            #[cfg(not(unix))]
            future::pending::<()>().await;
        } => {}
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::bail;
    use tokio::sync::Notify;
    use tokio::sync::oneshot;

    use super::*;

    #[tokio::test]
    async fn test_empty() {
        // The empty service should exit immediately.
        Service::new()
            .wait_for_shutdown(GRACE, std::future::pending)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_empty_attach_merge() {
        // Attaching and merging empty services should work without issue.
        Service::new()
            .attach(Service::new())
            .merge(Service::new())
            .wait_for_shutdown(GRACE, std::future::pending)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_completion() {
        let (atx, arx) = oneshot::channel::<()>();
        let (btx, brx) = oneshot::channel::<()>();

        let svc = Service::new().spawn(async move {
            let _brx = brx;
            Ok(arx.await?)
        });

        // The task has not finished yet (dropping the receiver)
        assert!(!btx.is_closed());

        // Sending the signal allows the task to complete successfully, which allows the service to
        // exit, and at that point, the second channel should be closed too.
        atx.send(()).unwrap();
        svc.wait_for_shutdown(GRACE, std::future::pending)
            .await
            .unwrap();
        assert!(btx.is_closed());
    }

    #[tokio::test]
    async fn test_failure() {
        let svc = Service::new().spawn(async move { bail!("boom") });
        let res = svc.wait_for_shutdown(GRACE, std::future::pending).await;
        assert!(matches!(res, Err(Error::Task(_))));
    }

    #[tokio::test]
    #[should_panic]
    async fn test_panic() {
        let svc = Service::new().spawn(async move { panic!("boom") });
        let _ = svc.wait_for_shutdown(GRACE, std::future::pending).await;
    }

    #[tokio::test]
    async fn test_graceful_shutdown() {
        let (atx, arx) = oneshot::channel::<()>();
        let (btx, brx) = oneshot::channel::<()>();

        let srx = Arc::new(Notify::new());
        let stx = srx.clone();

        let svc = Service::new()
            .with_shutdown_signal(async move { atx.send(()).unwrap() })
            .spawn(async move {
                arx.await?;
                btx.send(()).unwrap();
                Ok(())
            });

        // The service is now running in the background.
        let handle =
            tokio::spawn(svc.wait_for_shutdown(GRACE, move || srx.clone().notified_owned()));

        // Send the shutdown signal, and confirm the task went through its graceful shutdwon
        // process.
        stx.notify_one();
        brx.await.unwrap();

        // The service should exit gracefully now, dropping the receiver it was holding.
        let res = handle.await.unwrap();
        assert!(matches!(res, Err(Error::Terminated)));
    }

    #[tokio::test]
    async fn test_multiple_tasks() {
        let (atx, arx) = oneshot::channel::<()>();
        let (btx, brx) = oneshot::channel::<()>();
        let (ctx, crx) = oneshot::channel::<()>();

        // Three different tasks each waiting on a oneshot channel. We should be able to unblock
        // each of them before the service exits.
        let svc = Service::new()
            .spawn(async move { Ok(arx.await?) })
            .spawn(async move { Ok(brx.await?) })
            .spawn(async move { Ok(crx.await?) });

        let handle = tokio::spawn(svc.wait_for_shutdown(GRACE, std::future::pending));

        atx.send(()).unwrap();
        tokio::task::yield_now().await;

        btx.send(()).unwrap();
        tokio::task::yield_now().await;

        ctx.send(()).unwrap();
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_multiple_task_failure() {
        let (atx, arx) = oneshot::channel::<()>();

        // The task waiting on the channel (that aborts on shutdown) will never get to finish because
        // the other task errors out.
        let svc = Service::new()
            .spawn_aborting(async move { Ok(arx.await?) })
            .spawn(async move { bail!("boom") });

        let handle = tokio::spawn(svc.wait_for_shutdown(GRACE, std::future::pending));
        let res = handle.await.unwrap();

        // The service exits with a task error because one of the tasks failed, and this also
        // means the other task is aborted before it can complete successfully.
        assert!(matches!(res, Err(Error::Task(_))));
        assert!(atx.is_closed());
    }

    #[tokio::test]
    async fn test_secondary_stuck() {
        let (atx, arx) = oneshot::channel::<()>();
        let (btx, brx) = oneshot::channel::<()>();

        // A secondary task and a primary task.
        let snd = Service::new().spawn_aborting(async move { Ok(brx.await?) });
        let svc = Service::new()
            .spawn(async move { Ok(arx.await?) })
            .attach(snd);

        let handle = tokio::spawn(svc.wait_for_shutdown(GRACE, std::future::pending));

        // Complete the primary task, and the service as a whole should wrap up.
        atx.send(()).unwrap();
        handle.await.unwrap().unwrap();
        assert!(btx.is_closed());
    }

    #[tokio::test]
    async fn test_secondary_complete() {
        let (atx, arx) = oneshot::channel::<()>();
        let (btx, brx) = oneshot::channel::<()>();
        let (mut ctx, crx) = oneshot::channel::<()>();

        // A secondary task and a primary task.
        let snd = Service::new().spawn(async move {
            let _crx = crx;
            brx.await?;
            Ok(())
        });

        let svc = Service::new()
            .spawn(async move { Ok(arx.await?) })
            .attach(snd);

        let handle = tokio::spawn(svc.wait_for_shutdown(GRACE, std::future::pending));

        // Complete the secondary task, and wait for it to complete (dropping the other channel).
        btx.send(()).unwrap();
        ctx.closed().await;
        tokio::task::yield_now().await;

        // The primary task will not have been cleaned up, so we can send to it, completing that
        // task, and allowing the service as a whole to complete.
        atx.send(()).unwrap();
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_secondary_failure() {
        let (atx, arx) = oneshot::channel::<()>();

        // A secondary task that fails, with a primary task.
        let snd = Service::new().spawn(async move { bail!("boom") });
        let svc = Service::new()
            .spawn_aborting(async move { Ok(arx.await?) })
            .attach(snd);

        // Run the service -- it should fail immediately because the secondary task failed,
        // cleaning up the primary task.
        let res = svc.wait_for_shutdown(GRACE, std::future::pending).await;
        assert!(matches!(res, Err(Error::Task(_))));
        assert!(atx.is_closed());
    }

    #[tokio::test]
    #[should_panic]
    async fn test_secondary_panic() {
        let (_atx, arx) = oneshot::channel::<()>();

        // A secondary task that fails, with a primary task.
        let snd = Service::new().spawn(async move { panic!("boom") });
        let svc = Service::new()
            .spawn_aborting(async move { Ok(arx.await?) })
            .attach(snd);

        // When the service runs, the panic from the secondary task will be propagated.
        let _ = svc.wait_for_shutdown(GRACE, std::future::pending).await;
    }

    #[tokio::test]
    async fn test_secondary_graceful_shutdown() {
        let (atx, arx) = oneshot::channel::<()>();
        let (btx, brx) = oneshot::channel::<()>();
        let (ctx, crx) = oneshot::channel::<()>();

        let srx = Arc::new(Notify::new());
        let stx = srx.clone();

        // A secondary task with a shutdown signal.
        let snd = Service::new()
            .with_shutdown_signal(async move { atx.send(()).unwrap() })
            .spawn(async move {
                let _crx = crx;
                Ok(arx.await?)
            });

        // A primary task which aborts when signalled to shutdown.
        let svc = Service::new()
            .spawn_aborting(async move { Ok(brx.await?) })
            .attach(snd);

        // The service is now running in the background.
        let handle =
            tokio::spawn(svc.wait_for_shutdown(GRACE, move || srx.clone().notified_owned()));

        // Confirm that each task is still waiting on their respective channels.
        assert!(!btx.is_closed());
        assert!(!ctx.is_closed());

        // Send the shutdown signal - both tasks should be unblocked and complete gracefully.
        stx.notify_one();
        let res = handle.await.unwrap();
        assert!(matches!(res, Err(Error::Terminated)));
        assert!(btx.is_closed());
        assert!(ctx.is_closed());
    }

    #[tokio::test]
    async fn test_merge() {
        let (atx, arx) = oneshot::channel::<()>();
        let (btx, brx) = oneshot::channel::<()>();
        let (ctx, crx) = oneshot::channel::<()>();
        let (dtx, drx) = oneshot::channel::<()>();
        let (etx, erx) = oneshot::channel::<()>();
        let (ftx, frx) = oneshot::channel::<()>();

        let srx = Arc::new(Notify::new());
        let stx = srx.clone();

        // Set-up two services -- each with a task that can be shutdown, and a task that's paused,
        // which will send a message once unpaused.
        let a = Service::new()
            .spawn(async move { Ok(arx.await?) })
            .with_shutdown_signal(async move { ctx.send(()).unwrap() })
            .spawn(async move {
                crx.await?;
                dtx.send(()).unwrap();
                Ok(())
            });

        let b = Service::new()
            .spawn(async move { Ok(brx.await?) })
            .with_shutdown_signal(async move { etx.send(()).unwrap() })
            .spawn(async move {
                erx.await?;
                ftx.send(()).unwrap();
                Ok(())
            });

        // Merge them into a larger service and run it.
        let svc = Service::new().merge(a).merge(b);
        let handle =
            tokio::spawn(svc.wait_for_shutdown(GRACE, move || srx.clone().notified_owned()));

        // Unblock the paused tasks, so they terminate.
        atx.send(()).unwrap();
        tokio::task::yield_now().await;

        btx.send(()).unwrap();
        tokio::task::yield_now().await;

        // Send the shutdown signal - triggering graceful shutdown on the remaining tasks --
        // confirm that those tasks actually go through the graceful shutdown process.
        stx.notify_one();
        drx.await.unwrap();
        frx.await.unwrap();

        // ...and then the service shuts down.
        let res = handle.await.unwrap();
        assert!(matches!(res, Err(Error::Terminated)));
    }

    #[tokio::test]
    async fn test_drop_abort() {
        let (mut atx, arx) = oneshot::channel::<()>();
        let (mut btx, brx) = oneshot::channel::<()>();

        let svc = Service::new()
            .spawn(async move { Ok(arx.await?) })
            .spawn_aborting(async move { Ok(brx.await?) });

        assert!(!atx.is_closed());
        assert!(!btx.is_closed());

        // Once the service is dropped, its tasks will also be dropped, and the receivers will be
        // dropped, closing the senders.
        drop(svc);
        atx.closed().await;
        btx.closed().await;
    }

    #[tokio::test]
    async fn test_shutdown() {
        let (atx, arx) = oneshot::channel::<()>();
        let (btx, brx) = oneshot::channel::<()>();

        let svc = Service::new()
            .with_shutdown_signal(async move { atx.send(()).unwrap() })
            .spawn(async move { Ok(arx.await?) })
            .spawn_aborting(async move { Ok(brx.await?) });

        // We don't need to call `Service::run` to kick off the service's tasks -- they are now
        // running in the background. We can call `shutdown` to trigger a graceful shutdown, which
        // should exit cleanly and clean up any unused resources.
        svc.shutdown().await.unwrap();
        assert!(btx.is_closed());
    }

    #[tokio::test]
    async fn test_error_cascade() {
        let (atx, arx) = oneshot::channel::<()>();

        // The first task errors immediately, and the second task errors during graceful shutdown.
        let svc = Service::new()
            .spawn(async move { bail!("boom") })
            .with_shutdown_signal(async move { atx.send(()).unwrap() })
            .spawn(async move {
                arx.await?;
                bail!("boom, again")
            });

        // The service will exit with an abort.
        let res = svc.wait_for_shutdown(GRACE, std::future::pending).await;
        assert!(matches!(res, Err(Error::Aborted)));
    }

    #[tokio::test]
    async fn test_multiple_errors() {
        // Both tasks produce an error, one will be picked up during normal execution, and the
        // other will be picked up during shutdown, resulting in an ungraceful shutdown (abort).
        let svc = Service::new()
            .spawn(async move { bail!("boom") })
            .spawn(async move { bail!("boom, again") });

        // The service will exit with an abort.
        let res = svc.wait_for_shutdown(GRACE, std::future::pending).await;
        assert!(matches!(res, Err(Error::Aborted)));
    }

    #[tokio::test]
    async fn test_termination_cascade() {
        // A service with a task that ignores graceful shutdown.
        let svc = Service::new().spawn(std::future::pending());

        let srx = Arc::new(Notify::new());
        let stx = srx.clone();

        // The service is now running in the background.
        let handle =
            tokio::spawn(svc.wait_for_shutdown(GRACE, move || srx.clone().notified_owned()));

        // Trigger the first termination, which the task will ignore.
        stx.notify_one();
        tokio::task::yield_now().await;

        // Trigger the second termination, the service takes over.
        stx.notify_one();
        tokio::task::yield_now().await;

        let res = handle.await.unwrap();
        assert!(matches!(res, Err(Error::Aborted)));
    }

    #[tokio::test]
    #[should_panic]
    async fn test_panic_during_shutdown() {
        let (atx, arx) = oneshot::channel::<()>();

        let srx = Arc::new(Notify::new());
        let stx = srx.clone();

        let svc = Service::new()
            .with_shutdown_signal(async move { atx.send(()).unwrap() })
            .spawn(async move {
                arx.await?;
                panic!("boom")
            });

        // The service is now running in the background.
        let handle =
            tokio::spawn(svc.wait_for_shutdown(GRACE, move || srx.clone().notified_owned()));

        // Send the shutdown signal, the panic gets resumed when the service is awaited.
        stx.notify_one();
        let _ = handle.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn test_graceful_shutdown_timeout() {
        let srx = Arc::new(Notify::new());
        let stx = srx.clone();

        // A service with a task that ignores graceful shutdown.
        let svc = Service::new().spawn(std::future::pending());

        let handle =
            tokio::spawn(svc.wait_for_shutdown(GRACE, move || srx.clone().notified_owned()));

        // Trigger cancellation and then let twice the grace period pass, which should be treated
        // as an abort.
        stx.notify_one();
        tokio::time::advance(GRACE * 2).await;

        let res = handle.await.unwrap();
        assert!(matches!(res, Err(Error::Aborted)));
    }
}
