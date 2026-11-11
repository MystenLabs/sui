// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use parking_lot::RwLock;
use prometheus::{
    IntCounterVec, IntGauge, Registry, register_int_counter_vec_with_registry,
    register_int_gauge_with_registry,
};
use sui_protocol_config::assert_reachable_gated;
use sui_types::{
    accumulator_root::{AccumulatorObjId, UnsettledObjectFundsRead},
    base_types::SequenceNumber,
    digests::ChainIdentifier,
    effects::{TransactionEffects, TransactionEffectsAPI},
    execution_status::ExecutionStatus,
    funds_accumulator::is_object_funds_insufficient_abort,
    transaction::{TransactionData, TransactionDataAPI},
};

pub struct UnsettledObjectWithdrawals {
    inner: RwLock<Inner>,
    metrics: UnsettledObjectWithdrawalsMetrics,
}

struct UnsettledObjectWithdrawalsMetrics {
    unsettled_accounts: IntGauge,
    unsettled_versions: IntGauge,
    check_result: IntCounterVec,
}

impl UnsettledObjectWithdrawalsMetrics {
    fn new(registry: &Registry) -> Self {
        Self {
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
                "object_funds_in_execution_check_result",
                "Executed transactions by in-execution object funds check outcome: sufficient \
                 counts successful transactions that drew on an object account's settled balance",
                &["result"],
                registry,
            )
            .unwrap(),
        }
    }
}

#[derive(Default)]
struct Inner {
    /// Tracks the amount of pending unsettled withdraws for each account at each accumulator version.
    /// When we check object funds sufficiency, we read the balance bounded by the withdraw accumulator version.
    /// Balance are updated only by settlement transactions, not when we withdraw funds.
    /// Hence when we are checking object funds, on top of the settled balance, we also need to account for
    /// the amount of withdraws from the same consensus commit (that all reads from the same accumulator version).
    /// The recorded amounts are the per-account net withdraws from effects (what settlement will
    /// actually deduct).
    unsettled_withdraws: BTreeMap<AccumulatorObjId, BTreeMap<SequenceNumber, u128>>,
    /// Tracks the accounts that have pending withdraws at each accumulator version.
    /// This information is not required for functional correctness, but needed to garbage collect
    /// unused entries in unsettled_withdraws that are now fully committed. Without doing so unsettled_withdraws
    /// may grow unbounded.
    unsettled_accounts: BTreeMap<SequenceNumber, BTreeSet<AccumulatorObjId>>,
}

impl UnsettledObjectFundsRead for UnsettledObjectWithdrawals {
    fn get_unsettled_object_withdraw(
        &self,
        account: &AccumulatorObjId,
        accumulator_version: SequenceNumber,
    ) -> u128 {
        self.inner
            .read()
            .unsettled_withdraws
            .get(account)
            .and_then(|withdraws| withdraws.get(&accumulator_version))
            .copied()
            .unwrap_or_default()
    }
}

impl UnsettledObjectWithdrawals {
    pub fn new(registry: &Registry) -> Self {
        Self {
            inner: RwLock::new(Inner::default()),
            metrics: UnsettledObjectWithdrawalsMetrics::new(registry),
        }
    }

    /// Record a new unsettled withdraw for an account at a given accumulator version.
    /// This updates the unsettled withdraws map, and is called after a transaction successfully executed
    /// that withdraws funds from an object balance account.
    fn record_unsettled_withdraws(
        &self,
        withdraws: impl IntoIterator<Item = (AccumulatorObjId, u128)>,
        accumulator_version: SequenceNumber,
    ) {
        let mut inner = self.inner.write();
        for (account, amount) in withdraws {
            let entry = inner
                .unsettled_withdraws
                .entry(account)
                .or_default()
                .entry(accumulator_version)
                .or_default();
            *entry = entry.checked_add(amount).unwrap();
            inner
                .unsettled_accounts
                .entry(accumulator_version)
                .or_default()
                .insert(account);
        }
        self.update_unsettled_metrics(&inner);
    }

    /// Records the unsettled object withdraws of an executed transaction and counts its
    /// in-execution object funds check outcome.
    pub fn record_object_funds_withdraws(
        &self,
        tx_data: &TransactionData,
        effects: &TransactionEffects,
        accumulator_running_max_withdraws: &BTreeMap<AccumulatorObjId, u128>,
        accumulator_version: SequenceNumber,
        chain_identifier: ChainIdentifier,
    ) {
        match effects.status() {
            ExecutionStatus::Success => {
                if self.record_net_withdraws(
                    tx_data,
                    effects,
                    accumulator_running_max_withdraws,
                    accumulator_version,
                    chain_identifier,
                ) {
                    self.metrics
                        .check_result
                        .with_label_values(&["sufficient"])
                        .inc();
                }
            }
            ExecutionStatus::Failure(failure) => {
                if is_object_funds_insufficient_abort(&failure.error) {
                    assert_reachable_gated!("object funds insufficient in execution", |pc| pc
                        .check_object_funds_withdraw_in_execution());
                    self.metrics
                        .check_result
                        .with_label_values(&["insufficient"])
                        .inc();
                }
            }
        }
    }

    /// Returns whether the transaction drew on any object account's settled balance.
    fn record_net_withdraws(
        &self,
        tx_data: &TransactionData,
        effects: &TransactionEffects,
        accumulator_running_max_withdraws: &BTreeMap<AccumulatorObjId, u128>,
        accumulator_version: SequenceNumber,
        chain_identifier: ChainIdentifier,
    ) -> bool {
        if accumulator_running_max_withdraws.is_empty() {
            return false;
        }
        let address_funds_reservations: BTreeSet<_> = tx_data
            .process_funds_withdrawals_for_execution(chain_identifier)
            .into_keys()
            .collect();
        let object_accounts: BTreeSet<_> = accumulator_running_max_withdraws
            .keys()
            .filter(|account| !address_funds_reservations.contains(account))
            .collect();
        if object_accounts.is_empty() {
            return false;
        }
        let updates: Vec<_> = effects
            .accumulator_events()
            .into_iter()
            .filter(|event| object_accounts.contains(&event.accumulator_obj))
            .filter_map(|event| {
                event
                    .write
                    .get_fund_withdraw_amount()
                    .filter(|amount| *amount > 0)
                    .map(|amount| (event.accumulator_obj, amount))
            })
            .collect();
        if updates.is_empty() {
            return true;
        }
        debug_assert!(
            updates.iter().all(|(obj_id, net)| {
                accumulator_running_max_withdraws
                    .get(obj_id)
                    .is_some_and(|max| net <= max)
            }),
            "net withdraw exceeds running max: tx={:?} updates={:?} running_max={:?}",
            tx_data.digest(),
            updates,
            accumulator_running_max_withdraws,
        );
        self.record_unsettled_withdraws(updates, accumulator_version);
        assert_reachable_gated!(
            "record unsettled object withdraws from in-execution check",
            |pc| pc.check_object_funds_withdraw_in_execution()
        );
        true
    }

    fn update_unsettled_metrics(&self, inner: &Inner) {
        self.metrics
            .unsettled_accounts
            .set(inner.unsettled_withdraws.len() as i64);
        self.metrics
            .unsettled_versions
            .set(inner.unsettled_accounts.len() as i64);
    }

    /// Garbage collect tracking of unsettled withdraws for committed accumulator versions.
    /// This isn't required for functional correctness, but ensures that the tracking data structure doesn't grow unbounded.
    pub fn commit_accumulator_versions(&self, committed_accumulator_versions: Vec<SequenceNumber>) {
        let mut inner = self.inner.write();
        for accumulator_version in committed_accumulator_versions {
            let accounts = inner
                .unsettled_accounts
                .remove(&accumulator_version)
                .unwrap_or_default();
            for account in accounts {
                if let Some(withdraws) = inner.unsettled_withdraws.get_mut(&account) {
                    withdraws.remove(&accumulator_version);
                    if withdraws.is_empty() {
                        inner.unsettled_withdraws.remove(&account);
                    }
                }
            }
        }
        self.update_unsettled_metrics(&inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_core_types::language_storage::ModuleId;
    use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
    use sui_types::{
        accumulator_event::AccumulatorEvent,
        accumulator_root::AccumulatorValue,
        balance::Balance,
        base_types::{SuiAddress, random_object_ref},
        crypto::get_account_key_pair,
        effects::{
            AccumulatorAddress, AccumulatorOperation, AccumulatorValue as EffectsAccumulatorValue,
            AccumulatorWriteV1, TestEffectsBuilder,
        },
        execution_status::{ExecutionFailure, ExecutionFailureStatus, MoveLocation},
        funds_accumulator::{E_OBJECT_FUNDS_INSUFFICIENT, FUNDS_ACCUMULATOR_MODULE_NAME},
        gas_coin::GAS,
        transaction::Transaction,
        utils::create_fake_transaction,
    };

    fn sui_balance() -> sui_types::TypeTag {
        Balance::type_tag(GAS::type_tag())
    }

    fn account(owner: SuiAddress) -> AccumulatorObjId {
        AccumulatorValue::get_field_id(owner, &sui_balance()).unwrap()
    }

    fn split(owner: SuiAddress, amount: u64) -> AccumulatorEvent {
        AccumulatorEvent::new(
            account(owner),
            AccumulatorWriteV1 {
                address: AccumulatorAddress::new(owner, sui_balance()),
                operation: AccumulatorOperation::Split,
                value: EffectsAccumulatorValue::Integer(amount),
            },
        )
    }

    fn new_store() -> UnsettledObjectWithdrawals {
        UnsettledObjectWithdrawals::new(&Registry::new())
    }

    fn check_result(store: &UnsettledObjectWithdrawals, result: &str) -> u64 {
        store
            .metrics
            .check_result
            .with_label_values(&[result])
            .get()
    }

    /// Records a successful transaction and returns whether it counted as sufficient.
    fn record(
        store: &UnsettledObjectWithdrawals,
        tx: &Transaction,
        events: Vec<AccumulatorEvent>,
        running_max: BTreeMap<AccumulatorObjId, u128>,
        version: u64,
    ) -> bool {
        let effects = TestEffectsBuilder::new(tx.data())
            .with_accumulator_events(events)
            .build();
        let sufficient_before = check_result(store, "sufficient");
        store.record_object_funds_withdraws(
            tx.transaction_data(),
            &effects,
            &running_max,
            SequenceNumber::from_u64(version),
            ChainIdentifier::default(),
        );
        check_result(store, "sufficient") > sufficient_before
    }

    fn unsettled(store: &UnsettledObjectWithdrawals, owner: SuiAddress, version: u64) -> u128 {
        store.get_unsettled_object_withdraw(&account(owner), SequenceNumber::from_u64(version))
    }

    #[test]
    fn records_net_withdraw_not_running_max() {
        // Withdraw 100 then deposit 70 back: running max 100, net Split(30) in effects.
        let store = new_store();
        let owner = SuiAddress::random_for_testing_only();
        assert!(record(
            &store,
            &create_fake_transaction(),
            vec![split(owner, 30)],
            BTreeMap::from([(account(owner), 100)]),
            0,
        ));
        assert_eq!(unsettled(&store, owner, 0), 30);

        // A second transaction at the same version adds to the first.
        assert!(record(
            &store,
            &create_fake_transaction(),
            vec![split(owner, 30)],
            BTreeMap::from([(account(owner), 30)]),
            0,
        ));
        assert_eq!(unsettled(&store, owner, 0), 60);
    }

    #[test]
    fn net_zero_withdraw_records_nothing_but_counts_as_sufficient() {
        // Withdraw 100 then deposit 100 back: running max 100, no net withdraw in effects.
        let store = new_store();
        let owner = SuiAddress::random_for_testing_only();
        assert!(record(
            &store,
            &create_fake_transaction(),
            vec![],
            BTreeMap::from([(account(owner), 100)]),
            0,
        ));
        assert_eq!(unsettled(&store, owner, 0), 0);
    }

    #[test]
    fn zero_amount_split_is_skipped() {
        // A zero-amount withdraw survives effects folding as Split(0) but has no running max
        // entry.
        let store = new_store();
        let zero_owner = SuiAddress::random_for_testing_only();
        let owner = SuiAddress::random_for_testing_only();
        assert!(record(
            &store,
            &create_fake_transaction(),
            vec![split(zero_owner, 0), split(owner, 50)],
            BTreeMap::from([(account(owner), 50)]),
            0,
        ));
        assert_eq!(unsettled(&store, zero_owner, 0), 0);
        assert_eq!(unsettled(&store, owner, 0), 50);
    }

    #[test]
    fn no_withdraw_is_not_counted() {
        let store = new_store();
        assert!(!record(
            &store,
            &create_fake_transaction(),
            vec![],
            BTreeMap::new(),
            0
        ));
    }

    #[test]
    fn address_only_withdraw_records_nothing_and_is_not_counted() {
        // A withdraw from an address balance declared in transaction data is not an object
        // withdraw, even though it appears in the running max and in the effects.
        let store = new_store();
        let (sender, keypair) = get_account_key_pair();
        let tx = TestTransactionBuilder::new(sender, random_object_ref(), 1)
            .transfer_sui_to_address_balance(
                FundSource::address_fund_with_reservation(100),
                vec![(100, sender)],
            )
            .build_and_sign(&keypair);
        assert!(!record(
            &store,
            &tx,
            vec![split(sender, 100)],
            BTreeMap::from([(account(sender), 100)]),
            0,
        ));
        assert_eq!(unsettled(&store, sender, 0), 0);
    }

    #[test]
    fn mixed_withdraw_records_only_the_object_account() {
        // A transaction that withdraws from its sender's address balance and from an object
        // records only the object withdraw as unsettled.
        let store = new_store();
        let (sender, keypair) = get_account_key_pair();
        let object_owner = SuiAddress::random_for_testing_only();
        let tx = TestTransactionBuilder::new(sender, random_object_ref(), 1)
            .transfer_sui_to_address_balance(
                FundSource::address_fund_with_reservation(100),
                vec![(100, sender)],
            )
            .build_and_sign(&keypair);
        assert!(record(
            &store,
            &tx,
            vec![split(sender, 100), split(object_owner, 40)],
            BTreeMap::from([(account(sender), 100), (account(object_owner), 40)]),
            0,
        ));
        assert_eq!(unsettled(&store, sender, 0), 0);
        assert_eq!(unsettled(&store, object_owner, 0), 40);
    }

    #[test]
    fn insufficient_abort_records_nothing_and_counts_insufficient() {
        let store = new_store();
        let owner = SuiAddress::random_for_testing_only();
        let tx = create_fake_transaction();
        let abort = ExecutionFailureStatus::MoveAbort(
            MoveLocation {
                module: ModuleId::new(
                    sui_types::SUI_FRAMEWORK_ADDRESS,
                    FUNDS_ACCUMULATOR_MODULE_NAME.to_owned(),
                ),
                function: 0,
                instruction: 0,
                function_name: None,
            },
            E_OBJECT_FUNDS_INSUFFICIENT,
        );
        let effects = TestEffectsBuilder::new(tx.data())
            .with_status(ExecutionStatus::new_failure(ExecutionFailure::new(
                abort, None,
            )))
            .build();
        store.record_object_funds_withdraws(
            tx.transaction_data(),
            &effects,
            &BTreeMap::from([(account(owner), 100)]),
            SequenceNumber::from_u64(0),
            ChainIdentifier::default(),
        );
        assert_eq!(check_result(&store, "insufficient"), 1);
        assert_eq!(check_result(&store, "sufficient"), 0);
        assert_eq!(unsettled(&store, owner, 0), 0);
    }

    #[test]
    fn commit_removes_only_committed_versions() {
        let store = new_store();
        let owner = SuiAddress::random_for_testing_only();
        for version in [0, 1] {
            record(
                &store,
                &create_fake_transaction(),
                vec![split(owner, 10)],
                BTreeMap::from([(account(owner), 10)]),
                version,
            );
        }

        store.commit_accumulator_versions(vec![SequenceNumber::from_u64(0)]);
        assert_eq!(unsettled(&store, owner, 0), 0);
        assert_eq!(unsettled(&store, owner, 1), 10);

        store.commit_accumulator_versions(vec![SequenceNumber::from_u64(1)]);
        let inner = store.inner.read();
        assert!(inner.unsettled_withdraws.is_empty());
        assert!(inner.unsettled_accounts.is_empty());
    }
}
