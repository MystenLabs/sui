// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
    Authorities have a passive component (in AuthorityState), but can also have active
    components to perform a number of functions such as:

    (1) Share transactions received with other authorities, to complete their execution
        in case clients fail before sharing a transaction with sufficient authorities.
    (2) Share certificates with other authorities in case clients fail before a
        certificate has its execution finalized.
    (3) Gossip executed certificates digests with other authorities through following
        each other and using push / pull to execute certificates.
    (4) Perform the active operations necessary to progress the periodic checkpointing
        protocol.

    This component manages the root of all these active processes. It spawns services
    and tasks that actively initiate network operations to progress all these
    processes.

    Some ground rules:
    - The logic here does nothing "privileged", namely any process that could not
      have been performed over the public authority interface by an untrusted
      client.
    - All logic here should be safe to the ActiveAuthority state being transient
      and multiple instances running in parallel per authority, or at untrusted
      clients. Or Authority state being stopped, without its state being saved
      (loss of store), and then restarted some time later.

*/

use futures::{join, Future};
use std::{
    borrow::Borrow,
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Duration,
};
use sui_types::{
    base_types::AuthorityName,
    error::{SuiError, SuiResult},
};
use tokio::sync::Mutex;
use tracing::{debug, error};

use crate::{
    authority::AuthorityState, authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
};
use tokio::time::Instant;

pub mod gossip;
use gossip::gossip_process;

pub mod checkpoint_driver;
use checkpoint_driver::checkpoint_process;

use self::checkpoint_driver::CheckpointProcessControl;

// TODO: Make these into a proper config
const MAX_RETRIES_RECORDED: u32 = 10;
const DELAY_FOR_1_RETRY_MS: u64 = 2_000;
const EXPONENTIAL_DELAY_BASIS: u64 = 2;
pub const MAX_RETRY_DELAY_MS: u64 = 30_000;

pub struct AuthorityHealth {
    // Records the number of retries
    pub retries: u32,
    // The instant after which we should contact this
    // authority again.
    pub no_contact_before: Instant,
}

impl Default for AuthorityHealth {
    fn default() -> AuthorityHealth {
        AuthorityHealth {
            retries: 0,
            no_contact_before: Instant::now(),
        }
    }
}

impl AuthorityHealth {
    /// Sets the no contact instant to be larger than what
    /// is currently recorded.
    pub fn set_no_contact_for(&mut self, period: Duration) {
        let future_instant = Instant::now() + period;
        if self.no_contact_before < future_instant {
            self.no_contact_before = future_instant;
        }
    }

    // Reset the no contact to no delay
    pub fn reset_no_contact(&mut self) {
        self.no_contact_before = Instant::now();
    }

    pub fn can_initiate_contact_now(&self) -> bool {
        let now = Instant::now();
        self.no_contact_before <= now
    }
}

#[derive(Clone)]
pub struct ActiveAuthority<A> {
    // The local authority state
    pub state: Arc<AuthorityState>,
    // The network interfaces to other authorities
    pub net: Arc<AuthorityAggregator<A>>,
    // Network health
    pub health: Arc<Mutex<HashMap<AuthorityName, AuthorityHealth>>>,
}

impl<A> ActiveAuthority<A> {
    pub fn new(
        authority: Arc<AuthorityState>,
        authority_clients: BTreeMap<AuthorityName, A>,
    ) -> SuiResult<Self> {
        let committee = authority.clone_committee();

        Ok(ActiveAuthority {
            health: Arc::new(Mutex::new(
                committee
                    .voting_rights
                    .iter()
                    .map(|(name, _)| (*name, AuthorityHealth::default()))
                    .collect(),
            )),
            state: authority,
            net: Arc::new(AuthorityAggregator::new(committee, authority_clients)),
        })
    }

    /// Returns the amount of time we should wait to be able to contact at least
    /// 2/3 of the nodes in the committee according to the `no_contact_before`
    /// instant stored in the authority health records. A network needs 2/3 stake
    /// live nodes, so before that we are unlikely to be able to make process
    /// even if we have a few connections.
    pub async fn minimum_wait_for_majority_honest_available(&self) -> Instant {
        let lock = self.health.lock().await;
        let (_, instant) = self.net.committee.robust_value(
            lock.iter().map(|(name, h)| (*name, h.no_contact_before)),
            // At least one honest node is at or above it.
            self.net.committee.quorum_threshold(),
        );
        instant
    }

    /// Adds one more retry to the retry counter up to MAX_RETRIES_RECORDED, and then increases
    /// the`no contact` value to DELAY_FOR_1_RETRY_MS * EXPONENTIAL_DELAY_BASIS ^ retries, up to
    /// a maximum delay of MAX_RETRY_DELAY_MS.
    pub async fn set_failure_backoff(&self, name: AuthorityName) {
        let mut lock = self.health.lock().await;
        let mut entry = lock.entry(name).or_default();
        entry.retries = u32::min(entry.retries + 1, MAX_RETRIES_RECORDED);
        let delay: u64 = u64::min(
            DELAY_FOR_1_RETRY_MS * u64::pow(EXPONENTIAL_DELAY_BASIS, entry.retries),
            MAX_RETRY_DELAY_MS,
        );
        entry.set_no_contact_for(Duration::from_millis(delay));
    }

    /// Resets retries to zero and sets no contact to zero delay.
    pub async fn set_success_backoff(&self, name: AuthorityName) {
        let mut lock = self.health.lock().await;
        let mut entry = lock.entry(name).or_default();
        entry.retries = 0;
        entry.reset_no_contact();
    }

    /// Checks given the current time if we should contact this authority, ie
    /// if we are past any `no contact` delay.
    pub async fn can_contact(&self, name: AuthorityName) -> bool {
        let mut lock = self.health.lock().await;
        let entry = lock.entry(name).or_default();
        entry.can_initiate_contact_now()
    }
}

impl<A> ActiveAuthority<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub async fn spawn_all_active_processes(self, receiver: LifecycleTaskHandler) {
        self.spawn_active_processes(receiver, true, true).await
    }

    /// Spawn all active tasks.
    pub async fn spawn_active_processes(
        self,
        receiver: LifecycleTaskHandler,
        gossip: bool,
        checkpoint: bool,
    ) {
        let gossip_receiver = receiver.clone();
        let gossip_locals = self.clone();
        let gossip_join = gossip_receiver.spawn("Gossip".to_string(), move || {
            let inner_locals = gossip_locals.clone();
            async move {
                if gossip {
                    gossip_process(&inner_locals, 4).await;
                }
                Ok(())
            }
        });

        let checkpoint_locals = self; // .clone();
        let checkpoint_join = receiver.spawn("Gossip".to_string(), move || {
            let inner_locals = checkpoint_locals.clone();
            async move {
                if checkpoint {
                    checkpoint_process(&inner_locals, &CheckpointProcessControl::default()).await;
                }
                Ok(())
            }
        });

        // Run concurrently and wait for both to quit.
        let (res_gossip, res_checkpoint) = join!(gossip_join, checkpoint_join);

        if let Err(err) = res_gossip {
            error!("Gossip exit: {}", err);
        }
        if let Err(err) = res_checkpoint {
            error!("Checkpoint exit: {}", err);
        }
    }
}

// The active authority start-update-shutdown facilities.

// The active authority runs in a separate task, and manages the tasks of separate active
// processes, with one sub-task per active process. These subtasks internally listen to a
// watch channel on which others may send a start, shutdown, or update signal. When this
// is received the tasks are restarted, or shut down.

use tokio::sync::watch;

pub enum LifecycleSignal {
    Start,
    Restart,
    Exit,
}

pub struct LifecycleSignalSender {
    sender: watch::Sender<LifecycleSignal>,
}

impl LifecycleSignalSender {
    pub fn new() -> (Self, LifecycleTaskHandler) {
        let (sender, receiver) = watch::channel(LifecycleSignal::Start);
        (
            LifecycleSignalSender { sender },
            LifecycleTaskHandler { receiver },
        )
    }

    pub async fn signal(&self, signal: LifecycleSignal) {
        // TODO: reflect on what to do with this error.
        let _ = self.sender.send(signal);
    }
}

#[derive(Clone)]
pub struct LifecycleTaskHandler {
    pub receiver: watch::Receiver<LifecycleSignal>,
}

impl LifecycleTaskHandler {
    pub async fn spawn<F, T>(mut self, description: String, fun: F) -> Result<(), SuiError>
    where
        F: Fn() -> T,
        T: Future + Send + 'static,
        <T as Future>::Output: Borrow<Result<(), SuiError>> + Send + 'static,
    {
        let mut handle: Option<tokio::task::JoinHandle<Result<(), SuiError>>> = None;
        loop {
            // flags for each iteration on whether we should
            // start, restart or exit the task.
            let mut start = false;
            let mut abort = false;
            let mut exit = false;

            // Either the task finishes, and we return the result, or
            // we get a signal to restart or stop that we execute.
            let signal = tokio::select! {
                signal = self.receiver.changed() => {
                    signal
                },

                // If the handle exists and returns a result, we return this result.
                // Note that the async { ... } is never polled if handle is not Some(...)
                result = async { handle.as_mut().unwrap().await }, if handle.is_some() => {
                    return result.map_err(|_err| SuiError::GenericAuthorityError { error: "Task cancelled".to_string() })?;
                }
            };

            match signal {
                Err(_err) => {
                    debug!("Closing active tasks, command channel was dropped.");
                    abort = true;
                    exit = true;
                }
                Ok(()) => {
                    // We have actually received a new signal.
                    match *self.receiver.borrow() {
                        LifecycleSignal::Start => {
                            debug!("Starting task: {}", description);
                            start = true;
                        }
                        LifecycleSignal::Restart => {
                            debug!("Restarting task: {}", description);
                            abort = true;
                            start = true;
                        }
                        LifecycleSignal::Exit => {
                            debug!("Exit task: {}", description);
                            abort = true;
                            exit = true;
                        }
                    }
                }
            }

            // First close / abort previous task.
            if abort {
                if let Some(join_handle) = handle {
                    join_handle.abort();
                    if let Ok(inner_res) = join_handle.await {
                        return inner_res;
                    }
                    // In case this is an error it is due to
                    // cancelling the task and we will either
                    // restart it or exit.
                }
                handle = None;
            }

            // Second potentially restart task
            if start {
                let fut = fun();
                handle = Some(tokio::task::spawn(
                    async move { fut.await.borrow().clone() },
                ));
            }

            // Third exit the loop.
            if exit {
                break;
            }
        }

        // If we have not returned a result yet, its because the task was
        // cancelled and no result was ever recorded.
        Err(SuiError::GenericAuthorityError {
            error: "Task cancelled".to_string(),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_signal() {
        let (sender, receiver) = LifecycleSignalSender::new();
        let arc_int = Arc::new(std::sync::Mutex::new(0usize));

        let arc_int_clone = arc_int.clone();
        let join = tokio::task::spawn(async move {
            receiver
                .spawn("Test task 1".to_string(), move || {
                    let inner_arc = arc_int_clone.clone();
                    async move {
                        println!("inner start");
                        *inner_arc.lock().unwrap() += 1;
                        tokio::time::sleep(Duration::from_secs(50)).await;
                        println!("inner end");
                        Ok(())
                    }
                })
                .await
        });

        sender.signal(LifecycleSignal::Start).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        sender.signal(LifecycleSignal::Restart).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        sender.signal(LifecycleSignal::Exit).await;
        assert!(join.await.unwrap().is_err());
        assert!(*arc_int.lock().unwrap() == 2);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_signal_complete() {
        let (sender, receiver) = LifecycleSignalSender::new();
        let arc_int = Arc::new(std::sync::Mutex::new(0usize));

        let arc_int_clone = arc_int.clone();
        let join = tokio::task::spawn(async move {
            receiver
                .spawn("Test task 1".to_string(), move || {
                    let inner_arc = arc_int_clone.clone();
                    async move {
                        println!("inner start");
                        *inner_arc.lock().unwrap() += 1;
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        println!("inner end");
                        Ok(())
                    }
                })
                .await
        });

        sender.signal(LifecycleSignal::Start).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        sender.signal(LifecycleSignal::Restart).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        sender.signal(LifecycleSignal::Exit).await;

        // It complete on the first iteration, so ...
        assert!(join.await.unwrap().is_ok());
        // .. the restart does nothing.
        assert!(*arc_int.lock().unwrap() == 1);
    }
}
