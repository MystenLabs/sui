// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Compatibility support for the post-execution object-funds check used by old executors.

use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use prometheus::{
    Histogram, IntCounterVec, IntGauge, Registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_gauge_with_registry,
};
use sui_types::{
    accumulator_root::AccumulatorObjId,
    base_types::SequenceNumber,
    digests::ChainIdentifier,
    effects::{TransactionEffects, TransactionEffectsAPI},
    execution_params::FundsWithdrawStatus,
    transaction::{TransactionData, TransactionDataAPI},
};
use tokio::sync::watch;

const SCHEDULING_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
];

pub struct ObjectFundsCheckerMetrics {
    pub highest_settled_version: IntGauge,
    pub pending_checks: IntGauge,
    pub pending_check_latency: Histogram,
    pub unsettled_accounts: IntGauge,
    pub unsettled_versions: IntGauge,
    pub check_result: IntCounterVec,
    pub in_execution_check_result: IntCounterVec,
}

impl ObjectFundsCheckerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            highest_settled_version: register_int_gauge_with_registry!(
                "object_funds_highest_settled_version",
                "Highest settled accumulator version",
                registry,
            )
            .unwrap(),
            pending_checks: register_int_gauge_with_registry!(
                "object_funds_pending_checks",
                "Number of pending unresolved object funds checks",
                registry,
            )
            .unwrap(),
            pending_check_latency: register_histogram_with_registry!(
                "object_funds_pending_check_latency",
                "Latency in seconds from pending check creation to resolution",
                SCHEDULING_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            unsettled_accounts: register_int_gauge_with_registry!(
                "object_funds_unsettled_accounts",
                "Number of accounts with unsettled withdraws",
                registry,
            )
            .unwrap(),
            unsettled_versions: register_int_gauge_with_registry!(
                "object_funds_unsettled_versions",
                "Number of versions with unsettled accounts",
                registry,
            )
            .unwrap(),
            check_result: register_int_counter_vec_with_registry!(
                "object_funds_check_result",
                "Count of object funds check results by outcome",
                &["result"],
                registry,
            )
            .unwrap(),
            in_execution_check_result: register_int_counter_vec_with_registry!(
                "object_funds_in_execution_check_result",
                "Count of committed in-execution object funds check outcomes by result",
                &["result"],
                registry,
            )
            .unwrap(),
        }
    }
}

pub trait LegacyObjectFundsAccess {
    fn get_account_amount_at_version(
        &self,
        account: &AccumulatorObjId,
        version: SequenceNumber,
    ) -> u128;

    fn get_unsettled_withdraw(&self, account: &AccumulatorObjId, version: SequenceNumber) -> u128;

    fn record_unsettled_withdraws(
        &self,
        withdraws: BTreeMap<AccumulatorObjId, u128>,
        version: SequenceNumber,
    );
}

pub struct LegacyObjectFundsRetry {
    retry: Pin<Box<dyn Future<Output = FundsWithdrawStatus> + Send + 'static>>,
    metrics: Arc<ObjectFundsCheckerMetrics>,
    _timer: prometheus::HistogramTimer,
}

impl Future for LegacyObjectFundsRetry {
    type Output = FundsWithdrawStatus;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.get_mut().retry.as_mut().poll(cx)
    }
}

impl Drop for LegacyObjectFundsRetry {
    fn drop(&mut self) {
        self.metrics.pending_checks.dec();
    }
}

pub enum LegacyObjectFundsOutcome {
    NoCheck,
    Commit,
    MissingAccumulatorVersion,
    Retry(LegacyObjectFundsRetry),
}

pub struct LegacyObjectFundsChecker {
    last_settled_version_sender: watch::Sender<SequenceNumber>,
    last_settled_version_receiver: watch::Receiver<SequenceNumber>,
    metrics: Arc<ObjectFundsCheckerMetrics>,
}

impl LegacyObjectFundsChecker {
    pub fn new(
        starting_accumulator_version: SequenceNumber,
        metrics: Arc<ObjectFundsCheckerMetrics>,
    ) -> Self {
        let (last_settled_version_sender, last_settled_version_receiver) =
            watch::channel(starting_accumulator_version);
        Self {
            last_settled_version_sender,
            last_settled_version_receiver,
            metrics,
        }
    }

    pub fn check(
        &self,
        object_running_max_withdraws: BTreeMap<AccumulatorObjId, u128>,
        unsettled_withdraw_updates: BTreeMap<AccumulatorObjId, u128>,
        accumulator_version: SequenceNumber,
        access: &dyn LegacyObjectFundsAccess,
    ) -> LegacyObjectFundsOutcome {
        let last_settled_version = *self.last_settled_version_receiver.borrow();
        if accumulator_version <= last_settled_version {
            for (account, amount) in &object_running_max_withdraws {
                let funds = access.get_account_amount_at_version(account, accumulator_version);
                let unsettled = access.get_unsettled_withdraw(account, accumulator_version);
                assert!(funds >= unsettled);
                if funds - unsettled < *amount {
                    return self.ready_retry(FundsWithdrawStatus::Insufficient);
                }
            }
            access.record_unsettled_withdraws(unsettled_withdraw_updates, accumulator_version);
            self.metrics
                .check_result
                .with_label_values(&["sufficient"])
                .inc();
            return LegacyObjectFundsOutcome::Commit;
        }

        let mut version_receiver = self.last_settled_version_sender.subscribe();
        self.pending_retry(async move {
            if version_receiver
                .wait_for(|version| *version >= accumulator_version)
                .await
                .is_err()
            {
                tracing::error!("Last settled accumulator version receiver channel closed");
            }
            FundsWithdrawStatus::MaybeSufficient
        })
    }

    pub fn check_transaction(
        &self,
        transaction_data: &TransactionData,
        effects: &TransactionEffects,
        accumulator_running_max_withdraws: &BTreeMap<AccumulatorObjId, u128>,
        accumulator_version: Option<SequenceNumber>,
        record_net_unsettled_withdraws: bool,
        chain_identifier: ChainIdentifier,
        access: &dyn LegacyObjectFundsAccess,
    ) -> LegacyObjectFundsOutcome {
        if effects.status().is_err() || accumulator_running_max_withdraws.is_empty() {
            return LegacyObjectFundsOutcome::NoCheck;
        }

        let address_funds_reservations =
            transaction_data.process_funds_withdrawals_for_execution(chain_identifier);
        let object_running_max_withdraws: BTreeMap<_, _> = accumulator_running_max_withdraws
            .iter()
            .filter(|(account, _)| !address_funds_reservations.contains_key(account))
            .map(|(&account, &amount)| (account, amount))
            .collect();
        if object_running_max_withdraws.is_empty() {
            return LegacyObjectFundsOutcome::NoCheck;
        }
        let Some(accumulator_version) = accumulator_version else {
            return LegacyObjectFundsOutcome::MissingAccumulatorVersion;
        };

        let unsettled_withdraw_updates = if record_net_unsettled_withdraws {
            effects
                .accumulator_events()
                .into_iter()
                .filter(|event| !address_funds_reservations.contains_key(&event.accumulator_obj))
                .filter_map(|event| {
                    event
                        .write
                        .get_fund_withdraw_amount()
                        .filter(|amount| *amount > 0)
                        .map(|amount| (event.accumulator_obj, amount))
                })
                .collect()
        } else {
            object_running_max_withdraws.clone()
        };
        debug_assert!(unsettled_withdraw_updates.iter().all(|(account, net)| {
            object_running_max_withdraws
                .get(account)
                .is_some_and(|maximum| net <= maximum)
        }));

        self.check(
            object_running_max_withdraws,
            unsettled_withdraw_updates,
            accumulator_version,
            access,
        )
    }

    fn ready_retry(&self, status: FundsWithdrawStatus) -> LegacyObjectFundsOutcome {
        self.pending_retry(async move { status })
    }

    fn pending_retry(
        &self,
        retry: impl Future<Output = FundsWithdrawStatus> + Send + 'static,
    ) -> LegacyObjectFundsOutcome {
        self.metrics.pending_checks.inc();
        let metrics = self.metrics.clone();
        let timer = metrics.pending_check_latency.start_timer();
        let retry = Box::pin(async move {
            let status = retry.await;
            if matches!(status, FundsWithdrawStatus::Insufficient) {
                metrics
                    .check_result
                    .with_label_values(&["insufficient"])
                    .inc();
            }
            status
        });
        LegacyObjectFundsOutcome::Retry(LegacyObjectFundsRetry {
            retry,
            metrics: self.metrics.clone(),
            _timer: timer,
        })
    }

    pub fn settle_accumulator_version(&self, next_accumulator_version: SequenceNumber) {
        self.last_settled_version_sender
            .send(next_accumulator_version)
            .unwrap();
        self.metrics
            .highest_settled_version
            .set(next_accumulator_version.value() as i64);
    }

    #[cfg(test)]
    pub fn current_accumulator_version(&self) -> SequenceNumber {
        *self.last_settled_version_receiver.borrow()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use prometheus::Registry;
    use sui_types::{
        accumulator_root::AccumulatorObjId,
        base_types::{ObjectID, SequenceNumber},
        execution_params::FundsWithdrawStatus,
    };

    use super::{
        LegacyObjectFundsAccess, LegacyObjectFundsChecker, LegacyObjectFundsOutcome,
        ObjectFundsCheckerMetrics,
    };

    #[derive(Default)]
    struct TestAccess {
        balances: BTreeMap<AccumulatorObjId, u128>,
        unsettled: Mutex<BTreeMap<(AccumulatorObjId, SequenceNumber), u128>>,
    }

    impl LegacyObjectFundsAccess for TestAccess {
        fn get_account_amount_at_version(
            &self,
            account: &AccumulatorObjId,
            _version: SequenceNumber,
        ) -> u128 {
            self.balances.get(account).copied().unwrap_or_default()
        }

        fn get_unsettled_withdraw(
            &self,
            account: &AccumulatorObjId,
            version: SequenceNumber,
        ) -> u128 {
            self.unsettled
                .lock()
                .unwrap()
                .get(&(*account, version))
                .copied()
                .unwrap_or_default()
        }

        fn record_unsettled_withdraws(
            &self,
            withdraws: BTreeMap<AccumulatorObjId, u128>,
            version: SequenceNumber,
        ) {
            let mut unsettled = self.unsettled.lock().unwrap();
            for (account, amount) in withdraws {
                *unsettled.entry((account, version)).or_default() += amount;
            }
        }
    }

    fn checker(version: u64) -> LegacyObjectFundsChecker {
        LegacyObjectFundsChecker::new(
            SequenceNumber::from(version),
            Arc::new(ObjectFundsCheckerMetrics::new(&Registry::new())),
        )
    }

    fn account(byte: u8) -> AccumulatorObjId {
        AccumulatorObjId::new_unchecked(ObjectID::from_single_byte(byte))
    }

    #[tokio::test]
    async fn commits_and_records_when_funds_are_sufficient() {
        let account = account(1);
        let access = TestAccess {
            balances: BTreeMap::from([(account, 100)]),
            ..Default::default()
        };
        let outcome = checker(7).check(
            BTreeMap::from([(account, 60)]),
            BTreeMap::from([(account, 40)]),
            SequenceNumber::from(7),
            &access,
        );

        assert!(matches!(outcome, LegacyObjectFundsOutcome::Commit));
        assert_eq!(
            access.get_unsettled_withdraw(&account, SequenceNumber::from(7)),
            40
        );
    }

    #[tokio::test]
    async fn retries_as_insufficient_after_subtracting_unsettled_funds() {
        let account = account(2);
        let access = TestAccess {
            balances: BTreeMap::from([(account, 100)]),
            unsettled: Mutex::new(BTreeMap::from([((account, SequenceNumber::from(7)), 50)])),
        };
        let LegacyObjectFundsOutcome::Retry(retry) = checker(7).check(
            BTreeMap::from([(account, 60)]),
            BTreeMap::from([(account, 60)]),
            SequenceNumber::from(7),
            &access,
        ) else {
            panic!("expected retry");
        };

        assert_eq!(retry.await, FundsWithdrawStatus::Insufficient);
    }

    #[tokio::test]
    async fn waits_for_settlement_before_retrying() {
        let checker = checker(6);
        let access = TestAccess::default();
        let LegacyObjectFundsOutcome::Retry(retry) = checker.check(
            BTreeMap::from([(account(3), 1)]),
            BTreeMap::from([(account(3), 1)]),
            SequenceNumber::from(7),
            &access,
        ) else {
            panic!("expected retry");
        };

        checker.settle_accumulator_version(SequenceNumber::from(7));
        assert_eq!(retry.await, FundsWithdrawStatus::MaybeSufficient);
    }
}
