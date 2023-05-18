// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::sync::Arc;
use sui_types::sui_system_state::{SuiSystemState, SuiSystemStateTrait};
use tokio::sync::broadcast::error::RecvError;
use tracing::{info, warn};

use crate::{
    authority::AuthorityStore,
    authority_aggregator::{AuthAggMetrics, AuthorityAggregator},
    authority_client::{AuthorityAPI, NetworkAuthorityClient},
    epoch::committee_store::CommitteeStore,
    safe_client::SafeClientMetricsBase,
};

use super::QuorumDriver;

#[async_trait]
pub trait ReconfigObserver<A: Clone> {
    async fn run(&mut self, quorum_driver: Arc<QuorumDriver<A>>);
    fn clone_boxed(&self) -> Box<dyn ReconfigObserver<A> + Send + Sync>;
}

/// A ReconfigObserver that subscribes to a reconfig channel of new committee.
/// This is used in TransactionOrchestrator.
pub struct OnsiteReconfigObserver {
    reconfig_rx: tokio::sync::broadcast::Receiver<SuiSystemState>,
    authority_store: Arc<AuthorityStore>,
    committee_store: Arc<CommitteeStore>,
    safe_client_metrics_base: SafeClientMetricsBase,
    auth_agg_metrics: AuthAggMetrics,
}

impl OnsiteReconfigObserver {
    pub fn new(
        reconfig_rx: tokio::sync::broadcast::Receiver<SuiSystemState>,
        authority_store: Arc<AuthorityStore>,
        committee_store: Arc<CommitteeStore>,
        safe_client_metrics_base: SafeClientMetricsBase,
        auth_agg_metrics: AuthAggMetrics,
    ) -> Self {
        Self {
            reconfig_rx,
            authority_store,
            committee_store,
            safe_client_metrics_base,
            auth_agg_metrics,
        }
    }

    async fn create_authority_aggregator_from_system_state(
        &self,
    ) -> AuthorityAggregator<NetworkAuthorityClient> {
        AuthorityAggregator::new_from_local_system_state(
            &self.authority_store,
            &self.committee_store,
            self.safe_client_metrics_base.clone(),
            self.auth_agg_metrics.clone(),
        )
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
            authority_store: self.authority_store.clone(),
            committee_store: self.committee_store.clone(),
            safe_client_metrics_base: self.safe_client_metrics_base.clone(),
            auth_agg_metrics: self.auth_agg_metrics.clone(),
        })
    }

    async fn run(&mut self, quorum_driver: Arc<QuorumDriver<NetworkAuthorityClient>>) {
        // A tiny optimization: when a very stale node just starts, the
        // channel may fill up committees quickly. Here we skip directly to
        // the last known committee by looking at SuiSystemState.
        let authority_agg = self.create_authority_aggregator_from_system_state().await;
        if authority_agg.committee.epoch > quorum_driver.current_epoch() {
            quorum_driver
                .update_validators(Arc::new(authority_agg))
                .await;
        }
        loop {
            match self.reconfig_rx.recv().await {
                Ok(system_state) => {
                    let committee = system_state.get_current_epoch_committee();
                    info!(
                        "Got reconfig message. New committee: {}",
                        committee.committee
                    );
                    if committee.epoch() > quorum_driver.current_epoch() {
                        let authority_agg =
                            self.create_authority_aggregator_from_system_state().await;
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
    A: AuthorityAPI + Send + Sync + Clone + 'static,
{
    fn clone_boxed(&self) -> Box<dyn ReconfigObserver<A> + Send + Sync> {
        Box::new(Self {})
    }

    async fn run(&mut self, _quorum_driver: Arc<QuorumDriver<A>>) {}
}
