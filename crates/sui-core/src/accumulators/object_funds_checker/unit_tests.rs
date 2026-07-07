// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for the implementation of the object funds withdraw scheduler.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    accumulator_root::AccumulatorObjId,
    base_types::{ObjectID, SequenceNumber, random_object_ref},
    crypto::get_account_key_pair,
    effects::TestEffectsBuilder,
    executable_transaction::VerifiedExecutableTransaction,
    execution_params::FundsWithdrawStatus,
    execution_status::{ExecutionErrorKind, ExecutionFailure, ExecutionStatus},
};

use crate::{
    accumulators::{
        funds_read::AccountFundsRead,
        object_funds_checker::{
            ObjectFundsChecker, ObjectFundsWithdrawStatus, metrics::ObjectFundsCheckerMetrics,
        },
    },
    authority::{
        ExecutionEnv, shared_object_version_manager::AssignedVersions,
        test_authority_builder::TestAuthorityBuilder,
    },
    execution_scheduler::funds_withdraw_scheduler::mock_funds_read::MockFundsRead,
};

/// Checks object funds where the recorded net withdraws equal the checked running max
/// withdraws, i.e. transactions whose withdraws are not offset by any deposits.
fn check_object_funds_same(
    checker: &ObjectFundsChecker,
    withdraws: BTreeMap<AccumulatorObjId, u128>,
    accumulator_version: SequenceNumber,
    funds_read: &dyn AccountFundsRead,
) -> ObjectFundsWithdrawStatus {
    checker.check_object_funds(
        withdraws.clone(),
        withdraws,
        accumulator_version,
        funds_read,
    )
}

#[tokio::test]
async fn test_sufficient_balance() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    assert!(matches!(status, ObjectFundsWithdrawStatus::SufficientFunds));
}

#[tokio::test]
async fn test_insufficient_balance() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    // Since the withdraw version is the same as the last settled version, the scheduler can immediately
    // decide that the balance is insufficient.
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);
}

#[tokio::test]
async fn test_pending_wait() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    // Attempt to withdraw 101 at version 2.
    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(2),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    checker.settle_accumulator_version(SequenceNumber::from_u64(1));
    // The wait won't finish until it observes version 2.
    assert!(
        tokio::time::timeout(Duration::from_secs(1), receiver)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn test_pending_then_sufficient() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(1),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    funds_read
        .settle_funds_changes(
            BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 1)]),
            SequenceNumber::from_u64(1),
        )
        .await;
    checker.settle_accumulator_version(SequenceNumber::from_u64(1));
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::MaybeSufficient);

    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(1),
        funds_read.as_ref(),
    );
    assert!(matches!(status, ObjectFundsWithdrawStatus::SufficientFunds));
}

#[tokio::test]
async fn test_pending_then_insufficient() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 102)]),
        SequenceNumber::from_u64(1),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    funds_read
        .settle_funds_changes(
            BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 1)]),
            SequenceNumber::from_u64(1),
        )
        .await;
    checker.settle_accumulator_version(SequenceNumber::from_u64(1));
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::MaybeSufficient);

    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 102)]),
        SequenceNumber::from_u64(1),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);
}

#[tokio::test]
async fn test_multiple_withdraws() {
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account1, 100), (account2, 100)]),
    ));
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([
            (AccumulatorObjId::new_unchecked(account1), 100),
            (AccumulatorObjId::new_unchecked(account2), 50),
        ]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    assert!(matches!(status, ObjectFundsWithdrawStatus::SufficientFunds));
}

#[tokio::test]
async fn test_settle_accumulator_version() {
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    checker.settle_accumulator_version(SequenceNumber::from_u64(1));
    assert_eq!(
        checker.get_current_accumulator_version(),
        SequenceNumber::from_u64(1)
    );
}

#[tokio::test]
async fn test_account_version_ahead_of_schedule() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    funds_read
        .settle_funds_changes(
            BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 1)]),
            SequenceNumber::from_u64(1),
        )
        .await;
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    let result = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = result else {
        panic!("Expected pending status");
    };
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);

    let result = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    assert!(matches!(result, ObjectFundsWithdrawStatus::SufficientFunds));
}

#[tokio::test]
async fn test_settle_ahead_of_schedule() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    checker.settle_accumulator_version(SequenceNumber::from_u64(1));
    let result = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = result else {
        panic!("Expected pending status");
    };
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);
}

#[tokio::test]
async fn test_check_out_of_order() {
    // Check a withdraw on account A at version 0,
    // then a withdraw on account B at version 1,
    // then a withdraw on account A at version 0 again.
    // This is valid since transactions touching different accounts can be executed in any order.
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account1, 100), (account2, 100)]),
    ));
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 100)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    assert!(matches!(status, ObjectFundsWithdrawStatus::SufficientFunds));
    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account2), 100)]),
        SequenceNumber::from_u64(1),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(_) = status else {
        panic!("Expected pending status");
    };
    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 100)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);
}

#[tokio::test]
async fn test_commit() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    assert!(checker.is_empty());
    check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    assert!(!checker.is_empty());
    checker.commit_accumulator_versions(vec![SequenceNumber::from_u64(0)]);
    assert!(checker.is_empty());
}

#[tokio::test]
async fn test_should_commit_early_exits() {
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    let state = TestAuthorityBuilder::new().build().await;
    let epoch_store = state.epoch_store_for_testing().clone();

    let (sender, keypair) = get_account_key_pair();
    let tx = VerifiedExecutableTransaction::new_for_testing(
        TestTransactionBuilder::new(sender, random_object_ref(), 1).build(),
        &keypair,
    );
    let account = ObjectID::random();
    let withdraws = BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]);

    // Normal path that triggers object funds check. Should not commit since insufficient funds.
    assert!(!checker.should_commit_object_funds_withdraws(
        &tx,
        &TestEffectsBuilder::new(tx.data()).build(),
        &withdraws,
        &ExecutionEnv::new().with_assigned_versions(AssignedVersions::new(
            vec![],
            Some(SequenceNumber::from_u64(0))
        )),
        state.get_account_funds_read(),
        state.execution_scheduler(),
        &epoch_store,
    ));

    // Failed execution should always commit.
    assert!(
        checker.should_commit_object_funds_withdraws(
            &tx,
            &TestEffectsBuilder::new(tx.data())
                .with_status(ExecutionStatus::new_failure(ExecutionFailure::new(
                    ExecutionErrorKind::FunctionNotFound,
                    None,
                )))
                .build(),
            &withdraws,
            &ExecutionEnv::new().with_assigned_versions(AssignedVersions::new(
                vec![],
                Some(SequenceNumber::from_u64(0))
            )),
            state.get_account_funds_read(),
            state.execution_scheduler(),
            &epoch_store,
        )
    );
}

#[tokio::test]
async fn test_net_withdraws_do_not_block_same_version_withdraws() {
    // A transaction whose running max withdraw is 100 but whose net withdraw is 0
    // (e.g. withdraw 100 then deposit 100 back) should not consume any of the
    // unsettled balance at its accumulator version.
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    let status = checker.check_object_funds(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
        BTreeMap::new(),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    assert!(matches!(status, ObjectFundsWithdrawStatus::SufficientFunds));

    // The full balance is still available at the same version.
    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    assert!(matches!(status, ObjectFundsWithdrawStatus::SufficientFunds));

    // Now the balance is fully reserved by the previous withdraw.
    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 1)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);
}

#[tokio::test]
async fn test_partial_net_withdraws() {
    // A transaction with running max withdraw of 100 and net withdraw of 30
    // (e.g. withdraw 100 then deposit 70 back) reserves only 30 of the
    // unsettled balance.
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    let status = checker.check_object_funds(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 30)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    assert!(matches!(status, ObjectFundsWithdrawStatus::SufficientFunds));

    // 70 remains available; a withdraw of 71 must fail.
    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 71)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);

    let status = check_object_funds_same(
        &checker,
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 70)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    assert!(matches!(status, ObjectFundsWithdrawStatus::SufficientFunds));
}
