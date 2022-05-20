// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
    Authorities have a passive component (in AuthorityState), but can also have active
    components to perform a number of functions such as:

    (1) Share transactions received with other authorities, to complete their execution
        in case clients fail before sharing a transaction with sufficient authorities.
    (2) Share certificates with other authorities in case clients fail before a
        certificate has its executon finalized.
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

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Duration,
};
use sui_types::{base_types::AuthorityName, error::SuiResult};
use tokio::sync::Mutex;

use crate::{
    authority::AuthorityState, authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityClient,
};
use tokio::time::Instant;

pub mod gossip;
use gossip::gossip_process;

// TODO: Make these into a proper config
const MAX_RETRIES_RECORDED: u32 = 10;
const DELAY_FOR_1_RETRY_MS: u64 = 2_000;
const EXPONENTIAL_DELAY_BASIS: u64 = 2;
const MAX_RETRY_DELAY_MS: u64 = 30_000;

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

    pub fn can_contact_now(&self) -> bool {
        self.no_contact_before < Instant::now()
    }
}

pub struct ActiveAuthority {
    // The local authority state
    pub state: Arc<AuthorityState>,
    // The network interfaces to other authorities
    pub net: Arc<AuthorityAggregator>,
    // Network health
    pub health: Arc<Mutex<HashMap<AuthorityName, AuthorityHealth>>>,
}

impl ActiveAuthority {
    pub fn new(
        authority: Arc<AuthorityState>,
        authority_clients: BTreeMap<AuthorityName, AuthorityClient>,
    ) -> SuiResult<Self> {
        let committee = authority.committee.clone();

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

    // Resets retries to zero and sets no contact to zero delay.
    pub async fn set_success_backoff(&self, name: AuthorityName) {
        let mut lock = self.health.lock().await;
        let mut entry = lock.entry(name).or_default();
        entry.retries = 0;
        entry.reset_no_contact();
    }

    // Checks given the current time if we should contact this authority, ie
    // if we are past any `no contact` delay.
    pub async fn can_contact(&self, name: AuthorityName) -> bool {
        let mut lock = self.health.lock().await;
        let entry = lock.entry(name).or_default();
        entry.can_contact_now()
    }
}

impl ActiveAuthority {
    // TODO: Active tasks go here + logic to spawn them all
    pub async fn spawn_all_active_processes(self) -> Option<()> {
        // Spawn a task to take care of gossip
        let _gossip_join = tokio::task::spawn(async move {
            gossip_process(&self, 4).await;
        });

        Some(())
    }
}
