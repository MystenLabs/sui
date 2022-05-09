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

use std::{collections::BTreeMap, sync::Arc};
use sui_types::{base_types::AuthorityName, error::SuiResult};

use crate::{
    authority::AuthorityState, authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
};

pub mod gossip;
use gossip::gossip_process;

pub struct ActiveAuthority<A> {
    // The local authority state
    pub state: Arc<AuthorityState>,
    // The network interfaces to other authorities
    pub net: Arc<AuthorityAggregator<A>>,
}

impl<A> ActiveAuthority<A> {
    pub fn new(
        authority: Arc<AuthorityState>,
        authority_clients: BTreeMap<AuthorityName, A>,
    ) -> SuiResult<Self> {
        let committee = authority.committee.clone();

        Ok(ActiveAuthority {
            state: authority,
            net: Arc::new(AuthorityAggregator::new(committee, authority_clients)),
        })
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
