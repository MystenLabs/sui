// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use prometheus::Registry;
use sui_types::transaction_executor::ProposerSelector as _;

use super::*;
use crate::authority_aggregator::AuthorityAggregatorBuilder;
use crate::test_authority_clients::MockAuthorityApi;
use crate::validator_client_monitor::{OperationFeedback, OperationType};

/// The driver spawns a reconfig task on construction; these tests never change epoch.
struct NoopReconfigObserver;

#[async_trait]
impl ReconfigObserver<MockAuthorityApi> for NoopReconfigObserver {
    async fn run(&mut self, _updatable: Arc<dyn AuthorityAggregatorUpdatable<MockAuthorityApi>>) {
        std::future::pending::<()>().await
    }

    fn clone_boxed(&self) -> Box<dyn ReconfigObserver<MockAuthorityApi> + Send + Sync> {
        Box::new(NoopReconfigObserver)
    }
}

fn make_driver(committee_size: usize) -> Arc<TransactionDriver<MockAuthorityApi>> {
    let registry = Registry::new();
    let auth_agg = Arc::new(
        AuthorityAggregatorBuilder::from_committee_size(committee_size)
            .build_mock_authority_aggregator(),
    );
    TransactionDriver::new(
        auth_agg,
        Arc::new(NoopReconfigObserver),
        Arc::new(TransactionDriverMetrics::new(&registry)),
        None,
        Arc::new(ValidatorClientMetrics::new(&registry)),
    )
}

/// Records a distinct latency per validator so the ranking is total, then publishes it the way
/// the health check loop would.
fn observe_latencies(driver: &TransactionDriver<MockAuthorityApi>) {
    let auth_agg = driver.authority_aggregator().load_full();
    for (i, validator) in auth_agg.committee.names().enumerate() {
        driver
            .client_monitor
            .record_interaction_result(OperationFeedback {
                authority_name: *validator,
                display_name: auth_agg.get_display_name(validator),
                operation: OperationType::SharedObjectFinality,
                ping_type: None,
                result: Ok(Duration::from_millis((i as u64 + 1) * 100)),
            });
    }
    driver
        .client_monitor
        .force_update_cached_latencies(&auth_agg);
}

/// Before any latency is observed the ranking is an arbitrary shuffle, so the transaction is left
/// unrestricted rather than pinned to three validators picked at random.
#[tokio::test]
async fn test_no_proposers_before_any_observation() {
    let driver = make_driver(4);
    assert!(driver.preferred_proposers(3).is_none());
}

#[tokio::test]
async fn test_preferred_proposers_are_bounded_sorted_and_in_committee() {
    let driver = make_driver(7);
    observe_latencies(&driver);

    let committee_size = driver.authority_aggregator().load().committee.num_members() as u32;
    let allowed = driver.preferred_proposers(3).unwrap();

    assert_eq!(allowed.proposers.len(), 3);
    assert!(allowed.proposers.iter().is_sorted_by(|a, b| a < b));
    assert!(allowed.proposers.iter().all(|i| *i < committee_size));
    assert_eq!(
        allowed.epoch,
        driver.authority_aggregator().load().committee.epoch()
    );
}

/// A committee smaller than `max` yields every validator rather than failing.
#[tokio::test]
async fn test_preferred_proposers_clamped_to_committee_size() {
    let driver = make_driver(2);
    observe_latencies(&driver);

    let allowed = driver.preferred_proposers(3).unwrap();
    assert_eq!(allowed.proposers.len(), 2);
}
