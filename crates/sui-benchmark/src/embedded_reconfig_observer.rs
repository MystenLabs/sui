// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use std::sync::Arc;
use sui_core::authority_aggregator::AuthorityAggregator;
use sui_core::quorum_driver::AuthorityAggregatorUpdatable;
use sui_core::{
    authority_client::NetworkAuthorityClient, quorum_driver::reconfig_observer::ReconfigObserver,
};
use sui_network::default_mysten_network_config;
use sui_types::sui_system_state::SuiSystemStateTrait;
use tracing::{error, info, trace};

/// A ReconfigObserver that polls validators periodically
/// to get new epoch information.
/// Caveat:
/// 1. it does not guarantee to insert every committee into
///    committee store. This is fine in scenarios such as
///    stress, but may not be suitable in some other cases.
/// 2. because of 1, if it misses intermediate committee(s)
///    and we happen to have a big committee rotation, it may
///    fail to get quorum on the latest committee info from
///    demissioned validators and then stop working.
///
/// Background: this is a temporary solution for stress before
/// we see fullnode reconfiguration stabilizes.
#[derive(Clone, Default)]
pub struct EmbeddedReconfigObserver {}

impl EmbeddedReconfigObserver {
    pub fn new() -> Self {
        Self {}
    }
}

impl EmbeddedReconfigObserver {
    pub async fn get_committee(
        &self,
        auth_agg: Arc<AuthorityAggregator<NetworkAuthorityClient>>,
    ) -> anyhow::Result<Arc<AuthorityAggregator<NetworkAuthorityClient>>> {
        // auth_agg and cur_epoch is consistently in each iteration,
        // assuming no other ReconfigObserver is working at the same time.
        let cur_epoch = auth_agg.committee.epoch();
        match auth_agg
            .get_latest_system_state_object_for_testing()
            .await
            .map(|state| state.get_current_epoch_committee())
        {
            Err(err) => Err(err),
            Ok(committee_info) => {
                let network_config = default_mysten_network_config();
                let new_epoch = committee_info.epoch();
                if new_epoch <= cur_epoch {
                    trace!(
                        cur_epoch,
                        new_epoch,
                        "Ignored Committee from a previous or current epoch",
                    );
                    return Ok(auth_agg);
                }
                info!(
                    cur_epoch,
                    new_epoch, "Observed a new epoch, attempting to reconfig: {committee_info}"
                );
                auth_agg
                    .recreate_with_net_addresses(committee_info, &network_config, false)
                    .map(Arc::new)
                    .map_err(|se| anyhow!("Failed to recreate due to: {:?}", se.to_string()))
            }
        }
    }
}

#[async_trait]
impl ReconfigObserver<NetworkAuthorityClient> for EmbeddedReconfigObserver {
    fn clone_boxed(&self) -> Box<dyn ReconfigObserver<NetworkAuthorityClient> + Send + Sync> {
        Box::new(self.clone())
    }

    async fn run(
        &mut self,
        updatable: Arc<dyn AuthorityAggregatorUpdatable<NetworkAuthorityClient>>,
    ) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            let auth_agg = updatable.authority_aggregator();
            match self.get_committee(auth_agg.clone()).await {
                Ok(new_auth_agg) => updatable.update_authority_aggregator(new_auth_agg),
                Err(err) => {
                    error!(
                        "Failed to recreate authority aggregator with committee: {}",
                        err
                    );
                    continue;
                }
            }
        }
    }
}
