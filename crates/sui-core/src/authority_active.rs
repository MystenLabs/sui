// Copyright (c) Mysten Labs, Inc.
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

use arc_swap::ArcSwap;
use prometheus::Registry;
use std::{collections::HashMap, ops::Deref, sync::Arc, time::Duration};
use sui_metrics::spawn_monitored_task;
use sui_types::{base_types::AuthorityName, error::SuiResult};
use tokio::{
    sync::{oneshot, Mutex, MutexGuard},
    task::JoinHandle,
    time::timeout,
};
use tracing::{debug, error, info, warn};

use crate::{
    authority::AuthorityState,
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    node_sync::{node_sync_process, NodeSyncHandle, NodeSyncState},
};
use futures::pin_mut;
use once_cell::sync::OnceCell;

use tap::TapFallible;

use tokio::time::Instant;
pub mod gossip;
use gossip::{gossip_process, GossipMetrics};

use crate::authority_client::NetworkAuthorityClientMetrics;

pub mod execution_driver;

use self::execution_driver::{execution_process, ExecutionDriverMetrics};

// TODO: Make these into a proper config
const MAX_RETRIES_RECORDED: u32 = 10;
const DELAY_FOR_1_RETRY_MS: u64 = 2_000;
const EXPONENTIAL_DELAY_BASIS: u64 = 2;
pub const MAX_RETRY_DELAY_MS: u64 = 30_000;

#[derive(Debug)]
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

struct NodeSyncProcessHandle(JoinHandle<()>, oneshot::Sender<()>);

pub struct ActiveAuthority<A> {
    // The local authority state
    pub state: Arc<AuthorityState>,

    // Handle that holds a channel connected to NodeSyncState, used to send sync requests
    // into NodeSyncState.
    node_sync_handle: OnceCell<NodeSyncHandle>,

    // JoinHandle for the tokio task that is running the NodeSyncState::start(), as well as a
    // cancel sender which can be used to terminate that task gracefully.
    node_sync_process: Arc<Mutex<Option<NodeSyncProcessHandle>>>,

    // The network interfaces to other authorities
    pub net: ArcSwap<AuthorityAggregator<A>>,
    // Network health
    pub health: Arc<Mutex<HashMap<AuthorityName, AuthorityHealth>>>,
    // Gossip Metrics including gossip between validators and
    // node sync process between fullnode and validators
    pub gossip_metrics: GossipMetrics,

    // This is only meaningful if A is of type NetworkAuthorityClient,
    // and stored here for reconfiguration purposes.
    pub network_metrics: Arc<NetworkAuthorityClientMetrics>,

    pub execution_driver_metrics: ExecutionDriverMetrics,
}

impl<A> ActiveAuthority<A> {
    pub fn new(
        authority: Arc<AuthorityState>,
        net: AuthorityAggregator<A>,
        prometheus_registry: &Registry,
    ) -> SuiResult<Self> {
        let committee = authority.clone_committee();

        let net = Arc::new(net);
        let network_metrics = net.network_client_metrics.clone();

        Ok(ActiveAuthority {
            health: Arc::new(Mutex::new(
                committee
                    .names()
                    .map(|name| (*name, AuthorityHealth::default()))
                    .collect(),
            )),
            state: authority,
            node_sync_handle: OnceCell::new(),
            node_sync_process: Default::default(),
            net: ArcSwap::from(net),
            gossip_metrics: GossipMetrics::new(prometheus_registry),
            network_metrics,
            execution_driver_metrics: ExecutionDriverMetrics::new(prometheus_registry),
        })
    }

    pub fn agg_aggregator(&self) -> Arc<AuthorityAggregator<A>> {
        self.net.load().clone()
    }

    pub fn new_with_ephemeral_storage_for_test(
        authority: Arc<AuthorityState>,
        net: AuthorityAggregator<A>,
    ) -> SuiResult<Self> {
        Self::new(authority, net, &Registry::new())
    }

    /// Returns the amount of time we should wait to be able to contact at least
    /// 2/3 of the nodes in the committee according to the `no_contact_before`
    /// instant stored in the authority health records. A network needs 2/3 stake
    /// live nodes, so before that we are unlikely to be able to make process
    /// even if we have a few connections.
    pub async fn minimum_wait_for_majority_honest_available(&self) -> Instant {
        let lock = self.health.lock().await;

        let health_overview: Vec<_> = lock
            .iter()
            .map(|(name, h)| (*name, h.retries, h.no_contact_before - Instant::now()))
            .collect();
        debug!(health_overview = ?health_overview, "Current validator health metrics");

        let (_, instant) = self.net.load().committee.robust_value(
            lock.iter().map(|(name, h)| (*name, h.no_contact_before)),
            // At least one honest node is at or above it.
            self.net.load().committee.quorum_threshold(),
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

impl<A> Clone for ActiveAuthority<A> {
    fn clone(&self) -> Self {
        ActiveAuthority {
            state: self.state.clone(),
            node_sync_handle: self.node_sync_handle.clone(),
            node_sync_process: self.node_sync_process.clone(),
            net: ArcSwap::from(self.net.load().clone()),
            health: self.health.clone(),
            gossip_metrics: self.gossip_metrics.clone(),
            network_metrics: self.network_metrics.clone(),
            execution_driver_metrics: self.execution_driver_metrics.clone(),
        }
    }
}

impl<A> ActiveAuthority<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn node_sync_handle(self: Arc<Self>) -> NodeSyncHandle {
        self.node_sync_handle
            .get_or_init(|| {
                let node_sync_state = Arc::new(NodeSyncState::new(self.clone()));

                NodeSyncHandle::new(node_sync_state, self.gossip_metrics.clone())
            })
            .clone()
    }

    /// Spawn gossip process
    pub async fn spawn_gossip_process(self: Arc<Self>, degree: usize) -> JoinHandle<()> {
        // Number of tasks at most "degree" and no more than committee - 1
        // (validators do not follow themselves for gossip)
        let committee = self.state.committee.load().deref().clone();
        let target_num_tasks = usize::min(committee.num_members() - 1, degree);

        spawn_monitored_task!(gossip_process(&self, target_num_tasks))
    }

    /// Restart the node sync process only if one currently exists.
    pub async fn respawn_node_sync_process(self: Arc<Self>) {
        let self_lock = self.clone();
        let lock_guard = self_lock.node_sync_process.lock().await;
        if lock_guard.is_some() {
            self.respawn_node_sync_process_impl(lock_guard).await
        } else {
            debug!("no active node sync process - not respawning");
        }
    }

    /// Start the node sync process.
    pub async fn spawn_node_sync_process(self: Arc<Self>) {
        let self_lock = self.clone();
        let lock_guard = self_lock.node_sync_process.lock().await;
        self.respawn_node_sync_process_impl(lock_guard).await
    }

    async fn cancel_node_sync_process_impl(
        lock_guard: &mut MutexGuard<'_, Option<NodeSyncProcessHandle>>,
    ) {
        if let Some(NodeSyncProcessHandle(join_handle, cancel_sender)) = lock_guard.take() {
            info!("sending cancel request to node sync task");
            let _ = cancel_sender
                .send(())
                .tap_err(|_| warn!("failed to request cancellation of node sync task"));

            pin_mut!(join_handle);

            // try to join the task, then kill it if it doesn't cancel on its own.
            info!("waiting node sync task to exit");
            if timeout(Duration::from_secs(1), &mut join_handle)
                .await
                .is_err()
            {
                error!("node sync task did not terminate on its own. aborting.");
                join_handle.abort();
                let _ = join_handle.await;
            }
        }
    }

    async fn respawn_node_sync_process_impl(
        self: Arc<Self>,
        mut lock_guard: MutexGuard<'_, Option<NodeSyncProcessHandle>>,
    ) {
        let epoch = self.state.committee.load().epoch;
        info!(?epoch, "respawn_node_sync_process");
        Self::cancel_node_sync_process_impl(&mut lock_guard).await;

        let (cancel_sender, cancel_receiver) = oneshot::channel();
        let aggregator = self.agg_aggregator();

        let node_sync_handle = self.clone().node_sync_handle();
        let node_sync_store = self.state.node_sync_store.clone();

        info!("spawning node sync task");
        let join_handle = spawn_monitored_task!(node_sync_process(
            node_sync_handle,
            node_sync_store,
            epoch,
            aggregator,
            cancel_receiver,
        ));

        *lock_guard = Some(NodeSyncProcessHandle(join_handle, cancel_sender));
    }

    pub async fn cancel_node_sync_process_for_tests(&self) {
        let mut lock_guard = self.node_sync_process.lock().await;
        Self::cancel_node_sync_process_impl(&mut lock_guard).await;
    }

    /// Spawn pending certificate execution process
    pub async fn spawn_execute_process(self: Arc<Self>) -> JoinHandle<()> {
        spawn_monitored_task!(execution_process(self))
    }
}
