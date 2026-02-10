// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for the implementation of the object funds withdraw scheduler.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    accumulator_root::AccumulatorObjId,
    base_types::{ObjectID, SequenceNumber, random_object_ref},
    crypto::get_account_key_pair,
    executable_transaction::VerifiedExecutableTransaction,
    execution_params::FundsWithdrawStatus,
    execution_status::{ExecutionFailureStatus, ExecutionStatus},
};

use crate::{
    accumulators::object_funds_checker::{
        ObjectFundsChecker, ObjectFundsWithdrawStatus, metrics::ObjectFundsCheckerMetrics,
    },
    authority::{
        ExecutionEnv, shared_object_version_manager::AssignedVersions,
        test_authority_builder::TestAuthorityBuilder,
    },
    execution_scheduler::funds_withdraw_scheduler::mock_funds_read::MockFundsRead,
};

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
    let status = checker.check_object_funds(
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
    let status = checker.check_object_funds(
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
    let status = checker.check_object_funds(
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
    let status = checker.check_object_funds(
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

    let status = checker.check_object_funds(
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
    let status = checker.check_object_funds(
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

    let status = checker.check_object_funds(
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
    let status = checker.check_object_funds(
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
    let result = checker.check_object_funds(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = result else {
        panic!("Expected pending status");
    };
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);

    let result = checker.check_object_funds(
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
    let result = checker.check_object_funds(
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
    let status = checker.check_object_funds(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 100)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    assert!(matches!(status, ObjectFundsWithdrawStatus::SufficientFunds));
    let status = checker.check_object_funds(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account2), 100)]),
        SequenceNumber::from_u64(1),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(_) = status else {
        panic!("Expected pending status");
    };
    let status = checker.check_object_funds(
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
    checker.check_object_funds(
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
        &ExecutionStatus::Success,
        &withdraws,
        &ExecutionEnv::new().with_assigned_versions(AssignedVersions::new(
            vec![],
            Some(SequenceNumber::from_u64(0))
        )),
        state.get_account_funds_read(),
        state.execution_scheduler(),
        &epoch_store,
    ));
    // Fastpath path transactions that have object funds withdraws must wait.
    assert!(!checker.should_commit_object_funds_withdraws(
        &tx,
        &ExecutionStatus::Success,
        &withdraws,
        &ExecutionEnv::new().with_assigned_versions(AssignedVersions::new(vec![], None)),
        state.get_account_funds_read(),
        state.execution_scheduler(),
        &epoch_store,
    ));

    // Failed execution should always commit.
    assert!(checker.should_commit_object_funds_withdraws(
        &tx,
        &ExecutionStatus::new_failure(ExecutionFailureStatus::FunctionNotFound, None,),
        &withdraws,
        &ExecutionEnv::new().with_assigned_versions(AssignedVersions::new(
            vec![],
            Some(SequenceNumber::from_u64(0))
        )),
        state.get_account_funds_read(),
        state.execution_scheduler(),
        &epoch_store,
    ));
}

#[tokio::test]
async fn test_track_object_funds() {
    let account = ObjectID::random();
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );
    assert!(checker.is_empty());

    checker.track_object_funds(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 50)]),
        SequenceNumber::from_u64(0),
    );
    assert!(!checker.is_empty());

    // Committing the version should clean up the tracked state.
    checker.commit_accumulator_versions(vec![SequenceNumber::from_u64(0)]);
    assert!(checker.is_empty());
}

#[tokio::test]
async fn test_track_object_funds_cumulative() {
    // Verify that track_object_funds accumulates correctly and affects subsequent checks.
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let checker = ObjectFundsChecker::new(
        SequenceNumber::from_u64(0),
        Arc::new(ObjectFundsCheckerMetrics::new(&prometheus::Registry::new())),
    );

    // Track a 60-unit withdrawal (without checking). Simulates a checkpoint-known-sufficient tx.
    checker.track_object_funds(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 60)]),
        SequenceNumber::from_u64(0),
    );

    // Now a subsequent check for 50 should fail because 60 + 50 > 100.
    let status = checker.check_object_funds(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 50)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status because cumulative 60+50=110 > 100");
    };
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);

    // But a check for 40 should succeed because 60 + 40 = 100 <= 100.
    let status = checker.check_object_funds(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 40)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    assert!(matches!(status, ObjectFundsWithdrawStatus::SufficientFunds));
}

#[tokio::test]
async fn test_should_commit_known_sufficient_skips_check() {
    // When funds_withdraw_status is Sufficient, should_commit should return true
    // immediately even if the accumulator version is not yet settled.
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

    // With MaybeSufficient (unknown from consensus) and an unsettled version,
    // the check would return false (pending).
    assert!(!checker.should_commit_object_funds_withdraws(
        &tx,
        &ExecutionStatus::Success,
        &withdraws,
        &ExecutionEnv::new().with_assigned_versions(AssignedVersions::new(
            vec![],
            Some(SequenceNumber::from_u64(2)),
        )),
        state.get_account_funds_read(),
        state.execution_scheduler(),
        &epoch_store,
    ));

    // With Sufficient (known from checkpoint), should return true immediately
    // even at an unsettled accumulator version.
    assert!(
        checker.should_commit_object_funds_withdraws(
            &tx,
            &ExecutionStatus::Success,
            &withdraws,
            &ExecutionEnv::new()
                .with_assigned_versions(AssignedVersions::new(
                    vec![],
                    Some(SequenceNumber::from_u64(2)),
                ))
                .with_sufficient_funds(),
            state.get_account_funds_read(),
            state.execution_scheduler(),
            &epoch_store,
        )
    );

    // The withdrawal should still be tracked in the checker's internal state.
    assert!(!checker.is_empty());
}

#[tokio::test]
async fn test_should_commit_known_sufficient_tracks_for_subsequent_checks() {
    // Verify that when using the Sufficient fast path, the tracked withdrawal
    // correctly affects subsequent MaybeSufficient (consensus) checks.
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
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
    let withdraws_80 = BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 80)]);

    // First: a checkpoint-known-sufficient transaction withdrawing 80 at settled version 0.
    assert!(
        checker.should_commit_object_funds_withdraws(
            &tx,
            &ExecutionStatus::Success,
            &withdraws_80,
            &ExecutionEnv::new()
                .with_assigned_versions(AssignedVersions::new(
                    vec![],
                    Some(SequenceNumber::from_u64(0)),
                ))
                .with_sufficient_funds(),
            &(Arc::new(MockFundsRead::new(
                SequenceNumber::from_u64(0),
                BTreeMap::from([(account, 100)]),
            )) as Arc<dyn crate::accumulators::funds_read::AccountFundsRead>),
            state.execution_scheduler(),
            &epoch_store,
        )
    );

    // Second: a consensus transaction trying to withdraw 30 at the same version.
    // 80 + 30 = 110 > 100, so this should fail the check.
    let status = checker.check_object_funds(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 30)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending because cumulative 80+30=110 > 100");
    };
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);

    // A 20-unit withdrawal should succeed: 80 + 20 = 100 <= 100.
    let status = checker.check_object_funds(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 20)]),
        SequenceNumber::from_u64(0),
        funds_read.as_ref(),
    );
    assert!(matches!(status, ObjectFundsWithdrawStatus::SufficientFunds));
}
