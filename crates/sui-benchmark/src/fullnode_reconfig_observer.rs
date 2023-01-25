// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_core::{
    authority_aggregator::{AuthAggMetrics, AuthorityAggregator},
    authority_client::NetworkAuthorityClient,
    epoch::committee_store::CommitteeStore,
    quorum_driver::{reconfig_observer::ReconfigObserver, QuorumDriver},
    safe_client::SafeClientMetricsBase,
};
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::committee::Committee;
use tracing::{debug, error, trace};

/// A ReconfigObserver that polls FullNode periodically
/// to get new epoch information.
/// Caveat: it does not guarantee to insert every committee
/// into committee store. This is fine in scenarios such
/// as stress, but may not be suitable in some other cases.
#[derive(Clone)]
pub struct FullNodeReconfigObserver {
    pub fullnode_client: SuiClient,
    committee_store: Arc<CommitteeStore>,
    safe_client_metrics_base: SafeClientMetricsBase,
    auth_agg_metrics: AuthAggMetrics,
}

impl FullNodeReconfigObserver {
    pub async fn new(
        fullnode_rpc_url: &str,
        committee_store: Arc<CommitteeStore>,
        safe_client_metrics_base: SafeClientMetricsBase,
        auth_agg_metrics: AuthAggMetrics,
    ) -> Self {
        Self {
            fullnode_client: SuiClientBuilder::default()
                .build(fullnode_rpc_url)
                .await
                .unwrap_or_else(|e| {
                    panic!(
                        "Can't create SuiClient with rpc url {fullnode_rpc_url}: {:?}",
                        e
                    )
                }),
            committee_store,
            safe_client_metrics_base,
            auth_agg_metrics,
        }
    }
}

#[async_trait]
impl ReconfigObserver<NetworkAuthorityClient> for FullNodeReconfigObserver {
    fn clone_boxed(&self) -> Box<dyn ReconfigObserver<NetworkAuthorityClient> + Send + Sync> {
        Box::new(self.clone())
    }

    async fn run(&mut self, quorum_driver: Arc<QuorumDriver<NetworkAuthorityClient>>) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            match self.fullnode_client.read_api().get_sui_system_state().await {
                Ok(sui_system_state) => {
                    let epoch_id = sui_system_state.epoch;
                    if epoch_id > quorum_driver.current_epoch() {
                        debug!(epoch_id, "Got SuiSystemState in newer epoch");
                        let new_committee = match self
                            .fullnode_client
                            .read_api()
                            .get_committee_info(Some(epoch_id))
                            .await
                        {
                            Ok(committee) if committee.committee_info.is_some() => {
                                // Safe to unwrap, checked above
                                Committee::new(committee.epoch, BTreeMap::from_iter(committee.committee_info.unwrap().into_iter())).unwrap_or_else(
                                    |e| panic!("Can't create a valid Committee given info returned from Full Node: {:?}", e)
                                )
                            }
                            other => {
                                error!(
                                    "Can't get CommitteeInfo {} from Full Node: {:?}",
                                    epoch_id, other
                                );
                                continue;
                            }
                        };
                        let _ = self.committee_store.insert_new_committee(&new_committee);
                        match AuthorityAggregator::new_from_system_state(
                            &sui_system_state,
                            &self.committee_store,
                            self.safe_client_metrics_base.clone(),
                            self.auth_agg_metrics.clone(),
                        ) {
                            Ok(auth_agg) => {
                                quorum_driver.update_validators(Arc::new(auth_agg)).await
                            }
                            Err(err) => error!(
                                "Can't create AuthorityAggregator from SuiSystemState: {:?}",
                                err
                            ),
                        }
                    } else {
                        trace!(
                            epoch_id,
                            "Ignored SystemState from a previous or current epoch",
                        );
                    }
                }
                Err(err) => error!("Can't get SuiSystemState from Full Node: {:?}", err,),
            }
        }
    }
}
