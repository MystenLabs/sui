// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::sync::Arc;
use sui_core::{
    authority_client::NetworkAuthorityClient,
    quorum_driver::{reconfig_observer::ReconfigObserver, QuorumDriver},
};
use sui_network::default_mysten_network_config;
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
/// Background: this is a temporary solution for stress before
/// we see fullnode reconfiguration stabilizes.
#[derive(Clone, Default)]
pub struct EmbeddedReconfigObserver {}

impl EmbeddedReconfigObserver {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl ReconfigObserver<NetworkAuthorityClient> for EmbeddedReconfigObserver {
    fn clone_boxed(&self) -> Box<dyn ReconfigObserver<NetworkAuthorityClient> + Send + Sync> {
        Box::new(self.clone())
    }

    async fn run(&mut self, quorum_driver: Arc<QuorumDriver<NetworkAuthorityClient>>) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

            // auth_agg and cur_epoch is consistently in each iteration,
            // assuming no other ReconfigObserver is working at the same time.
            let auth_agg = quorum_driver.authority_aggregator().load();
            let cur_epoch = quorum_driver.current_epoch();

            match auth_agg
                .get_committee_with_net_addresses(quorum_driver.current_epoch())
                .await
            {
                Err(err) => {
                    error!("Failed to get committee with network address: {}", err)
                }
                Ok(committee_info) => {
                    let network_config = default_mysten_network_config();
                    let new_epoch = committee_info.committee.epoch;
                    if new_epoch <= cur_epoch {
                        trace!(
                            cur_epoch,
                            new_epoch,
                            "Ignored Committee from a previous or current epoch",
                        );
                        continue;
                    }
                    info!(
                        cur_epoch,
                        new_epoch, "Observed a new epoch, attempting to reconfig: {committee_info}"
                    );
                    match auth_agg.recreate_with_net_addresses(
                        committee_info,
                        &network_config,
                        false,
                    ) {
                        Err(err) => error!(
                            cur_epoch,
                            new_epoch,
                            "Failed to recreate authority aggregator with committee: {}",
                            err
                        ),
                        Ok(auth_agg) => {
                            quorum_driver.update_validators(Arc::new(auth_agg)).await;
                            info!(cur_epoch, new_epoch, "Reconfiguration to epoch is done");
                        }
                    }
                }
            }
        }
    }
}
