// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::sync::Arc;
use sui_types::committee::CommitteeWithNetAddresses;
use tokio::sync::broadcast::error::RecvError;
use tracing::{info, warn};

use crate::{
    authority_aggregator::{AuthAggMetrics, AuthorityAggregator},
    authority_client::{AuthorityAPI, NetworkAuthorityClient},
    epoch::committee_store::CommitteeStore,
    safe_client::SafeClientMetricsBase,
};

use super::QuorumDriver;

#[async_trait]
pub trait ReconfigObserver<A> {
    async fn run(&mut self, quorum_driver: Arc<QuorumDriver<A>>);
    fn clone_boxed(&self) -> Box<dyn ReconfigObserver<A> + Send + Sync>;
}

/// A ReconfigObserver that subscribes to a reconfig channel of new committee.
/// This is used in TransactionOrchestrator.
pub struct OnsiteReconfigObserver {
    reconfig_rx: tokio::sync::broadcast::Receiver<CommitteeWithNetAddresses>,
    committee_store: Arc<CommitteeStore>,
    safe_client_metrics_base: SafeClientMetricsBase,
    auth_agg_metrics: AuthAggMetrics,
}

impl OnsiteReconfigObserver {
    pub fn new(
        reconfig_rx: tokio::sync::broadcast::Receiver<CommitteeWithNetAddresses>,
        committee_store: Arc<CommitteeStore>,
        safe_client_metrics_base: SafeClientMetricsBase,
        auth_agg_metrics: AuthAggMetrics,
    ) -> Self {
        Self {
            reconfig_rx,
            committee_store,
            safe_client_metrics_base,
            auth_agg_metrics,
        }
    }

    async fn create_authority_aggregator_from_new_committeee(
        &self,
        new_committee: CommitteeWithNetAddresses,
    ) -> AuthorityAggregator<NetworkAuthorityClient> {
        AuthorityAggregator::new_from_committee(
            new_committee,
            &self.committee_store,
            self.safe_client_metrics_base.clone(),
            self.auth_agg_metrics.clone(),
        )
        // TODO: we should tolerate when <= f validators give invalid addresses
        // GH issue: https://github.com/MystenLabs/sui/issues/7019
        .unwrap_or_else(|e| {
            panic!(
                "Failed to create AuthorityAggregator from System State: {:?}",
                e
            )
        })
    }
}

#[async_trait]
impl ReconfigObserver<NetworkAuthorityClient> for OnsiteReconfigObserver {
    fn clone_boxed(&self) -> Box<dyn ReconfigObserver<NetworkAuthorityClient> + Send + Sync> {
        Box::new(Self {
            reconfig_rx: self.reconfig_rx.resubscribe(),
            committee_store: self.committee_store.clone(),
            safe_client_metrics_base: self.safe_client_metrics_base.clone(),
            auth_agg_metrics: self.auth_agg_metrics.clone(),
        })
    }

    async fn run(&mut self, quorum_driver: Arc<QuorumDriver<NetworkAuthorityClient>>) {
        loop {
            match self.reconfig_rx.recv().await {
                Ok(committee) => {
                    info!("Got reconfig message: {}", committee);
                    if committee.committee.epoch > quorum_driver.current_epoch() {
                        let authority_agg = self
                            .create_authority_aggregator_from_new_committeee(committee)
                            .await;
                        quorum_driver
                            .update_validators(Arc::new(authority_agg))
                            .await;
                    } else {
                        // This should only happen when the node just starts
                        warn!("Epoch number decreased - ignoring committee: {}", committee);
                    }
                }
                // It's ok to miss messages due to overflow here
                Err(RecvError::Lagged(_)) => {
                    continue;
                }
                Err(RecvError::Closed) => panic!("Do not expect the channel to be closed"),
            }
        }
    }
}
/// A dummy ReconfigObserver for testing.
pub struct DummyReconfigObserver;

#[async_trait]
impl<A> ReconfigObserver<A> for DummyReconfigObserver
where
    A: AuthorityAPI + Send + Sync + 'static,
{
    fn clone_boxed(&self) -> Box<dyn ReconfigObserver<A> + Send + Sync> {
        Box::new(Self {})
    }

    async fn run(&mut self, _quorum_driver: Arc<QuorumDriver<A>>) {}
}
