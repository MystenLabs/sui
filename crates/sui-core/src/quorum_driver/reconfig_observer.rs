// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::AuthorityAggregatorUpdatable;
use crate::{
    authority_aggregator::AuthAggMetrics,
    authority_client::{AuthorityAPI, NetworkAuthorityClient},
    epoch::committee_store::CommitteeStore,
    execution_cache::ObjectCacheRead,
    safe_client::SafeClientMetricsBase,
};
use async_trait::async_trait;
use std::sync::Arc;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::sui_system_state::SuiSystemStateTrait;
use tokio::sync::broadcast::error::RecvError;
use tracing::{info, warn};

#[async_trait]
pub trait ReconfigObserver<A: Clone> {
    async fn run(&mut self, epoch_updatable: Arc<dyn AuthorityAggregatorUpdatable<A>>);
    fn clone_boxed(&self) -> Box<dyn ReconfigObserver<A> + Send + Sync>;
}

/// A ReconfigObserver that subscribes to a reconfig channel of new committee.
/// This is used in TransactionOrchestrator.
pub struct OnsiteReconfigObserver {
    reconfig_rx: tokio::sync::broadcast::Receiver<SuiSystemState>,
    execution_cache: Arc<dyn ObjectCacheRead>,
    committee_store: Arc<CommitteeStore>,
    // TODO: Use Arc for both metrics.
    safe_client_metrics_base: SafeClientMetricsBase,
    auth_agg_metrics: AuthAggMetrics,
}

impl OnsiteReconfigObserver {
    pub fn new(
        reconfig_rx: tokio::sync::broadcast::Receiver<SuiSystemState>,
        execution_cache: Arc<dyn ObjectCacheRead>,
        committee_store: Arc<CommitteeStore>,
        safe_client_metrics_base: SafeClientMetricsBase,
        auth_agg_metrics: AuthAggMetrics,
    ) -> Self {
        Self {
            reconfig_rx,
            execution_cache,
            committee_store,
            safe_client_metrics_base,
            auth_agg_metrics,
        }
    }
}

#[async_trait]
impl ReconfigObserver<NetworkAuthorityClient> for OnsiteReconfigObserver {
    fn clone_boxed(&self) -> Box<dyn ReconfigObserver<NetworkAuthorityClient> + Send + Sync> {
        Box::new(Self {
            reconfig_rx: self.reconfig_rx.resubscribe(),
            execution_cache: self.execution_cache.clone(),
            committee_store: self.committee_store.clone(),
            safe_client_metrics_base: self.safe_client_metrics_base.clone(),
            auth_agg_metrics: self.auth_agg_metrics.clone(),
        })
    }

    async fn run(
        &mut self,
        updatable: Arc<dyn AuthorityAggregatorUpdatable<NetworkAuthorityClient>>,
    ) {
        loop {
            match self.reconfig_rx.recv().await {
                Ok(system_state) => {
                    let epoch_start_state = system_state.into_epoch_start_state();
                    let committee = epoch_start_state.get_sui_committee();
                    info!("Got reconfig message. New committee: {}", committee);
                    if committee.epoch() > updatable.epoch() {
                        let new_auth_agg = updatable
                            .authority_aggregator()
                            .recreate_with_new_epoch_start_state(&epoch_start_state);
                        updatable.update_authority_aggregator(Arc::new(new_auth_agg));
                    } else {
                        // This should only happen when the node just starts
                        warn!("Epoch number decreased - ignoring committee: {}", committee);
                    }
                }
                // It's ok to miss messages due to overflow here
                Err(RecvError::Lagged(_)) => {
                    continue;
                }
                Err(RecvError::Closed) => {
                    // Closing the channel only happens in simtest when a node is shut down.
                    if cfg!(msim) {
                        return;
                    } else {
                        panic!("Do not expect the channel to be closed")
                    }
                }
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

    async fn run(&mut self, _quorum_driver: Arc<dyn AuthorityAggregatorUpdatable<A>>) {}
}
