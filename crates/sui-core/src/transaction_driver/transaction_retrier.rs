// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use itertools::Itertools as _;
use rand::seq::SliceRandom as _;
use sui_types::base_types::{AuthorityName, ConciseableName as _};

use crate::{
    authority_aggregator::AuthorityAggregator, safe_client::SafeClient,
    status_aggregator::StatusAggregator, transaction_driver::error::TransactionDriverError,
};

/// Provides the next target validator to retry operations,
/// and gathers the errors along with the operations.
///
/// In TransactionDriver, submitting a transaction and getting full effects follow the same pattern:
/// 1. Retry against all validators until the operation succeeds.
/// 2. If non‑retriable errors from a quorum of validators are returned, the operation should fail permanently.
///
/// This component helps to manager this retry pattern.
pub(crate) struct TransactionRetrier<A: Clone> {
    remaining_clients: Vec<(AuthorityName, Arc<SafeClient<A>>)>,
    non_retriable_errors_aggregator: StatusAggregator<TransactionDriverError>,
}

impl<A: Clone> TransactionRetrier<A> {
    pub(crate) fn new(auth_agg: &Arc<AuthorityAggregator<A>>) -> Self {
        // TODO(fastpath): select and order targets based on performance metrics.
        let mut remaining_clients = auth_agg
            .authority_clients
            .iter()
            .map(|(name, client)| (*name, client.clone()))
            .collect::<Vec<_>>();
        remaining_clients.shuffle(&mut rand::thread_rng());
        let non_retriable_errors_aggregator =
            StatusAggregator::<TransactionDriverError>::new(auth_agg.committee.clone());
        Self {
            remaining_clients,
            non_retriable_errors_aggregator,
        }
    }

    // Selects the next target validator to attempt an operation.
    pub(crate) fn next_target(
        &mut self,
    ) -> Result<(AuthorityName, Arc<SafeClient<A>>), TransactionDriverError> {
        let Some((name, client)) = self.remaining_clients.pop() else {
            // TODO(fastpath): aggregated saves errors.
            return Err(TransactionDriverError::NoMoreTargets);
        };
        Ok((name, client))
    }

    // Adds an error associated with the operation against the authority.
    //
    // Returns an error if it has aggregated >= f+1 submission non-retriable errors.
    // In this case, the transaction cannot finalize unless there is a software bug
    // or > f malicious validators.
    // TODO(fastpath): return an aggregated error.
    pub(crate) fn add_error(
        &mut self,
        name: AuthorityName,
        error: TransactionDriverError,
    ) -> Result<(), TransactionDriverError> {
        // TODO(fastpath): check if the error is non-retriable.
        self.non_retriable_errors_aggregator.insert(name, error);

        if self
            .non_retriable_errors_aggregator
            .reached_validity_threshold()
        {
            Err(TransactionDriverError::TransactionRejected(
                self.non_retriable_errors_aggregator.total_votes(),
                self.non_retriable_errors_aggregator
                    .statuses()
                    .iter()
                    .map(|(n, s)| format!("{}: {:?}", n.concise(), s))
                    .join(", "),
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Mutex, time::Duration};

    use fastcrypto::traits::KeyPair as _;
    use sui_types::{
        committee::Committee,
        crypto::{get_key_pair, AuthorityKeyPair},
    };

    use crate::{
        authority_aggregator::{AuthorityAggregatorBuilder, TimeoutConfig},
        test_authority_clients::MockAuthorityApi,
    };

    use super::*;

    fn get_authority_aggregator(committee_size: usize) -> AuthorityAggregator<MockAuthorityApi> {
        let count = Arc::new(Mutex::new(0));
        let mut authorities = BTreeMap::new();
        let mut clients = BTreeMap::new();
        for _ in 0..committee_size {
            let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
            let name: AuthorityName = sec.public().into();
            authorities.insert(name, 1);
            clients.insert(
                name,
                MockAuthorityApi::new(Duration::from_millis(100), count.clone()),
            );
        }

        let (committee, _keypairs) = Committee::new_simple_test_committee_of_size(committee_size);
        let timeouts_config = TimeoutConfig {
            serial_authority_request_interval: Duration::from_millis(50),
            ..Default::default()
        };
        AuthorityAggregatorBuilder::from_committee(committee)
            .with_timeouts_config(timeouts_config)
            .build_custom_clients(clients)
    }

    #[tokio::test]
    async fn test_next_target() {
        let auth_agg = Arc::new(get_authority_aggregator(4));
        let mut retrier = TransactionRetrier::new(&auth_agg);

        for _ in 0..4 {
            retrier.next_target().unwrap();
        }
        assert!(retrier.next_target().is_err());
    }

    #[tokio::test]
    async fn test_add_error() {
        let auth_agg = Arc::new(get_authority_aggregator(4));
        let authorities: Vec<_> = auth_agg.committee.names().copied().collect();
        let mut retrier = TransactionRetrier::new(&auth_agg);

        // 25% stake.
        retrier
            .add_error(authorities[0], TransactionDriverError::NoMoreTargets)
            .unwrap();
        // 50% stake, above validity threshold.
        retrier
            .add_error(authorities[1], TransactionDriverError::NoMoreTargets)
            .unwrap_err();
        // 75% stake, above validity threshold.
        retrier
            .add_error(authorities[2], TransactionDriverError::NoMoreTargets)
            .unwrap_err();
    }
}
