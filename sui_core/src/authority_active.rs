// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
    Authorities have a passive component (in AuthorityState), but can also have active
    components to perform a number of functions such as:

    (1) Share transactions received with other authorities, to complete their execution
        in case clients fail before sharing a trasnaction with sufficient authorities.
    (2) Share certificates with other authorities in case clients fail before a
        certificate has its executon finalized.
    (3) Gossip executed certificates digests with other authorities through following
        each other and using push / pull to execute certificates.
    (4) Perform the active operations necessary to progress the periodic checkpointing
        protocol.

    This component manages the root of all these active processes. It spawns services
    and tasks that activelly initiate network operations to progess all these
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
    authority_client::AuthorityAPI,
};
use tokio::time::Instant;

pub mod gossip;
use gossip::gossip_process;

pub struct AuthorityHealth {
    pub retries: u32,
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
    pub fn set_no_contact_for(&mut self, period: Duration) {
        let future_instant = Instant::now() + period;
        if self.no_contact_before < future_instant {
            self.no_contact_before = future_instant;
        }
    }

    pub fn can_contact_now(&self) -> bool {
        self.no_contact_before < Instant::now()
    }
}

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

    pub async fn minimum_wait_for_majority_honest_available(&self) -> Instant {
        let lock = self.health.lock().await;
        let (_, instant) = self.net.committee.robust_value(
            lock.iter().map(|(name, h)| (*name, h.no_contact_before)),
            // At least one honest node is at or above it.
            self.net.committee.quorum_threshold(),
        );
        instant
    }

    pub async fn set_failure_backoff(&self, name: AuthorityName) {
        let mut lock = self.health.lock().await;
        let mut entry = lock.entry(name).or_default();
        entry.retries = u32::min(entry.retries + 1, 10);
        let delay: u64 = u64::min(u64::pow(2, entry.retries), 180);
        entry.set_no_contact_for(Duration::from_secs(delay));
    }

    pub async fn set_success_backoff(&self, name: AuthorityName) {
        let mut lock = self.health.lock().await;
        let mut entry = lock.entry(name).or_default();
        entry.retries = 0;
        entry.set_no_contact_for(Duration::from_secs(0));
    }

    pub async fn can_contact(&self, name: AuthorityName) -> bool {
        let mut lock = self.health.lock().await;
        let entry = lock.entry(name).or_default();
        entry.can_contact_now()
    }
}

impl<A> ActiveAuthority<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // TODO: Active tasks go here + logic to spawn them all
    pub async fn spawn_all_active_processes(self) -> Option<()> {
        // Spawn a task to take care of gossip
        let _gossip_join = tokio::task::spawn(async move {
            gossip_process(&self, 4).await;
        });

        Some(())
    }
}
