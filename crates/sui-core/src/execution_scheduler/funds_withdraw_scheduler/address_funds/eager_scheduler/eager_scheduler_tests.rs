// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use mysten_metrics::monitored_mpsc::unbounded_channel;
use parking_lot::Mutex;
use rand::{Rng, seq::SliceRandom};
use sui_macros::sim_test;
use sui_types::{
    accumulator_root::AccumulatorObjId,
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
};
use tracing::{debug, info};

use crate::execution_scheduler::funds_withdraw_scheduler::{
    ScheduleStatus, TxFundsWithdraw,
    address_funds::{
        ScheduleResult,
        test_scheduler::{TestScheduler, expect_schedule_results},
    },
};

#[tokio::test]
async fn test_schedule_right_away() {
    // When we schedule withdraws at a version that is already settled,
    // we should immediately return the results.
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new_eager(init_version, BTreeMap::from([(account, 100)]));

    let withdraw1 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
    };
    let withdraw2 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 1)]),
    };

    let results = test.schedule_withdraws(init_version, vec![withdraw1.clone(), withdraw2.clone()]);
    expect_schedule_results(
        results,
        BTreeMap::from([
            (withdraw1.tx_digest, ScheduleStatus::SufficientFunds),
            (withdraw2.tx_digest, ScheduleStatus::InsufficientFunds),
        ]),
    );
}

#[tokio::test]
async fn test_already_settled() {
    let init_version = SequenceNumber::from_u64(0);
    let v1 = init_version.next();
    let account = ObjectID::random();
    let test = TestScheduler::new_eager(v1, BTreeMap::from([(account, 100)]));

    let withdraw = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
    };
    let results = test.schedule_withdraws(init_version, vec![withdraw.clone()]);
    expect_schedule_results(
        results,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SkipSchedule)]),
    );

    // Bump the underlying object version to v2.
    // Even though the scheduler itself is still at v0 as the last settled version,
    // withdrawing v1 is still considered as already settled since the object version is already at v2.
    let v2 = v1.next();
    test.mock_read
        .settle_funds_changes(
            BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 0)]),
            v2,
        )
        .await;
    let results = test.schedule_withdraws(v1, vec![withdraw.clone()]);
    expect_schedule_results(
        results,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SkipSchedule)]),
    );
}

#[tokio::test]
async fn test_withdraw_one_account_already_settled() {
    let init_version = SequenceNumber::from_u64(0);
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let test = TestScheduler::new_eager(
        init_version,
        BTreeMap::from([(account1, 100), (account2, 200)]),
    );

    // Advance one of the accounts to the next version, but not settle yet.
    test.mock_read
        .settle_funds_changes(
            BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 0)]),
            init_version.next(),
        )
        .await;

    let withdraw = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([
            (AccumulatorObjId::new_unchecked(account1), 50),
            (AccumulatorObjId::new_unchecked(account2), 100),
        ]),
    };

    let results = test.schedule_withdraws(init_version, vec![withdraw.clone()]);
    expect_schedule_results(
        results,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SkipSchedule)]),
    );
}

#[tokio::test]
async fn test_multiple_withdraws_same_version() {
    // This test checks that even though the second withdraw failed due to insufficient balance,
    // the third withdraw can still be scheduled since the second withdraw does not reserve any balance.
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new_eager(init_version, BTreeMap::from([(account, 90)]));

    let withdraw1 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 50)]),
    };
    let withdraw2 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 50)]),
    };
    let withdraw3 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 40)]),
    };

    let results = test.schedule_withdraws(
        init_version,
        vec![withdraw1.clone(), withdraw2.clone(), withdraw3.clone()],
    );
    expect_schedule_results(
        results,
        BTreeMap::from([
            (withdraw1.tx_digest, ScheduleStatus::SufficientFunds),
            (withdraw2.tx_digest, ScheduleStatus::InsufficientFunds),
            (withdraw3.tx_digest, ScheduleStatus::SufficientFunds),
        ]),
    );
}

#[tokio::test]
async fn test_multiple_withdraws_multiple_accounts_same_version() {
    let init_version = SequenceNumber::from_u64(0);
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let test = TestScheduler::new_eager(
        init_version,
        BTreeMap::from([(account1, 100), (account2, 100)]),
    );

    let withdraw1 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([
            (AccumulatorObjId::new_unchecked(account1), 100),
            (AccumulatorObjId::new_unchecked(account2), 200),
        ]),
    };
    let withdraw2 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 1)]),
    };
    let withdraw3 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account2), 100)]),
    };

    let results = test.schedule_withdraws(
        init_version,
        vec![withdraw1.clone(), withdraw2.clone(), withdraw3.clone()],
    );
    expect_schedule_results(
        results,
        BTreeMap::from([
            (withdraw1.tx_digest, ScheduleStatus::InsufficientFunds),
            (withdraw2.tx_digest, ScheduleStatus::InsufficientFunds),
            (withdraw3.tx_digest, ScheduleStatus::SufficientFunds),
        ]),
    );
}

#[tokio::test]
async fn test_withdraw_settle_and_deleted_account() {
    telemetry_subscribers::init_for_testing();
    let v0 = SequenceNumber::from_u64(0);
    let v1 = v0.next();
    let account = ObjectID::random();
    let account_id = AccumulatorObjId::new_unchecked(account);
    let scheduler = TestScheduler::new_eager(v0, BTreeMap::from([(account, 100)]));

    // Only update the account balance, without calling the scheduler to settle the balances.
    // This means that the scheduler still thinks we are at v0.
    // The settlement of -100 should lead to 0 balance, causing the account to be deleted.
    scheduler
        .mock_read
        .settle_funds_changes(BTreeMap::from([(account_id, -100)]), v1)
        .await;

    let withdraw = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account_id, 100)]),
    };

    let results = scheduler.schedule_withdraws(v0, vec![withdraw.clone()]);
    expect_schedule_results(
        results,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SkipSchedule)]),
    );
}

#[tokio::test]
async fn test_schedule_early_sufficient_funds() {
    // When an account has sufficient funds, even when the accumulator version is not yet at the version that the withdraw is scheduled for,
    // the withdraw should be scheduled immediately.
    let init_version = SequenceNumber::from_u64(0);
    let v1 = init_version.next();
    let account = ObjectID::random();
    let test = TestScheduler::new_eager(init_version, BTreeMap::from([(account, 100)]));
    let withdraw = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
    };
    let results = test.schedule_withdraws(v1, vec![withdraw.clone()]);
    assert!(matches!(
        results.get(&withdraw.tx_digest).unwrap(),
        ScheduleResult::ScheduleResult(ScheduleStatus::SufficientFunds)
    ));
}

#[tokio::test]
async fn test_schedule_multiple_accounts() {
    // Multiple accounts are reserved in the same transaction, one has sufficient funds, one does not.
    // The scheduler should reserve for one account and mark pending for the other.
    let init_version = SequenceNumber::from_u64(0);
    let v1 = init_version.next();
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let test = TestScheduler::new_eager(
        init_version,
        BTreeMap::from([(account1, 100), (account2, 100)]),
    );
    let withdraw1 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([
            (AccumulatorObjId::new_unchecked(account1), 50),
            (AccumulatorObjId::new_unchecked(account2), 101),
        ]),
    };
    let withdraw2 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 50)]),
    };
    let withdraw3 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 1)]),
    };
    let results = test.schedule_withdraws(
        v1,
        vec![withdraw1.clone(), withdraw2.clone(), withdraw3.clone()],
    );
    assert!(matches!(
        results.get(&withdraw1.tx_digest).unwrap(),
        ScheduleResult::Pending(_)
    ));
    assert!(matches!(
        results.get(&withdraw2.tx_digest).unwrap(),
        ScheduleResult::ScheduleResult(ScheduleStatus::SufficientFunds)
    ));
    assert!(matches!(
        results.get(&withdraw3.tx_digest).unwrap(),
        ScheduleResult::Pending(_)
    ));
}

#[tokio::test]
async fn test_schedule_settle() {
    let init_version = SequenceNumber::from_u64(0);
    let v1 = init_version.next();
    let account = ObjectID::random();
    let test = TestScheduler::new_eager(init_version, BTreeMap::from([(account, 100)]));
    let withdraw = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
    };
    let mut results = test.schedule_withdraws(v1, vec![withdraw.clone()]);
    let ScheduleResult::Pending(receiver) = results.remove(&withdraw.tx_digest).unwrap() else {
        panic!("Expected a pending result");
    };
    test.settle_funds_changes(v1, BTreeMap::from([(account, 1)]))
        .await;
    assert_eq!(test.scheduler.get_current_accumulator_version(), v1);
    let result = receiver.await.unwrap();
    assert_eq!(result, ScheduleStatus::SufficientFunds);
}

struct StressTestEnv {
    init_balances: BTreeMap<ObjectID, u128>,
    accounts: Vec<ObjectID>,
    withdraws: Vec<(SequenceNumber, Vec<TxFundsWithdraw>)>,
}

impl StressTestEnv {
    fn new(num_accounts: usize, num_transactions: usize) -> Self {
        let mut version = SequenceNumber::from_u64(0);
        let accounts = (0..num_accounts)
            .map(|_| ObjectID::random())
            .collect::<Vec<_>>();
        let mut rng = rand::thread_rng();
        let init_balances = accounts
            .iter()
            .filter_map(|account_id| {
                if rng.gen_bool(0.7) {
                    Some((*account_id, rng.gen_range(0..20)))
                } else {
                    None
                }
            })
            .collect::<BTreeMap<_, _>>();
        tracing::debug!("Init balances: {:?}", init_balances);

        let mut withdraws = Vec::new();
        let mut cur_reservations = Vec::new();
        for idx in 0..num_transactions {
            let num_reservation_accounts = rng.gen_range(1..3);
            let account_ids = accounts
                .choose_multiple(&mut rng, num_reservation_accounts)
                .cloned()
                .collect::<Vec<_>>();
            let reservations = account_ids
                .iter()
                .map(|account_id| {
                    (
                        AccumulatorObjId::new_unchecked(*account_id),
                        rng.gen_range(1..10),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            cur_reservations.push(TxFundsWithdraw {
                tx_digest: TransactionDigest::random(),
                reservations,
            });
            // Every now and then we group all withdraws into a commit, which we would
            // generate a settlement for later.
            if rng.gen_bool(0.2) || idx == num_transactions - 1 {
                withdraws.push((version, std::mem::take(&mut cur_reservations)));
                version = version.next();
            }
        }

        Self {
            init_balances,
            accounts,
            withdraws,
        }
    }
}

#[sim_test]
async fn balance_withdraw_scheduler_stress_test() {
    telemetry_subscribers::init_for_testing();

    let num_accounts = 5;
    let num_transactions = 20000;

    info!(
        "Running stress test with num_accounts: {:?}, num_transactions: {:?}",
        num_accounts, num_transactions
    );

    let StressTestEnv {
        init_balances,
        accounts,
        withdraws,
    } = StressTestEnv::new(num_accounts, num_transactions);

    info!("Starting stress test");

    // Repeat the process many times to ensure deterministic results.
    let mut expected_results: Option<BTreeMap<TransactionDigest, ScheduleStatus>> = None;
    let settlements = Arc::new(Mutex::new(Vec::new()));
    for test_run in 0..10 {
        debug!("Running test instance {:?}", test_run);
        let init_balances = init_balances.clone();
        let accounts = accounts.clone();
        let withdraws = withdraws.clone();
        let settlements = settlements.clone();

        let results = tokio::time::timeout(
            Duration::from_secs(60),
            async {
                let mut version = SequenceNumber::from_u64(0);
                let test = TestScheduler::new_eager(
                    version,
                    init_balances,
                );

                // Start a separate thread to run all settlements on the scheduler.
                let test_clone = test.clone();
                let (schedule_results_tx, mut schedule_results_rx) = unbounded_channel::<BTreeMap<AccumulatorObjId, u64>>("test");
                let settle_task = tokio::spawn(async move {
                    let mut idx = 0;
                    while let Some(reserved_amounts) = schedule_results_rx.recv().await {
                        if test_run == 0 {
                            // Only generate random settlements for the first test run.
                            // All future test runs should use the same settlements.
                            let mut rng = rand::thread_rng();
                            let num_changes = rng.gen_range(0..accounts.len());
                            let balance_changes = accounts
                                .choose_multiple(&mut rng, num_changes)
                                .map(|account_id| {
                                    let withdraws = if let Some(reserved_amount) = reserved_amounts.get(&AccumulatorObjId::new_unchecked(*account_id)) {
                                        rng.gen_range(0..*reserved_amount) as i128
                                    } else {
                                        0
                                    };
                                    let deposits = rng.gen_range(0..10) as i128;
                                    let change = deposits - withdraws;
                                    (*account_id, change)
                                })
                                .collect::<BTreeMap<_, _>>();
                            settlements.lock().push(balance_changes);
                        }

                        version = version.next();
                        let change = settlements.lock()[idx].clone();
                        test_clone.settle_funds_changes(version, change).await;
                        idx += 1;
                    }
                });

                let mut all_receivers = Vec::new();
                for (version, withdraws) in withdraws {
                    debug!("Test instance scheduling withdraws for version {:?}", version);
                    let receivers = test.schedule_withdraws(version, withdraws.clone());
                    all_receivers.push((version, receivers, withdraws));
                }

                let mut results = BTreeMap::new();
                for (version, receivers, withdraws) in all_receivers {
                    debug!("Test instance waiting for results from version {:?}, receiver count: {}", version, receivers.len());
                    for (tx_digest, result) in receivers {
                        let status = match result {
                            ScheduleResult::ScheduleResult(status) => status,
                            ScheduleResult::Pending(receiver) => receiver.await.unwrap(),
                        };
                        debug!("Test instance received result for tx {:?} with status {:?} at version {:?}", tx_digest, status, version);
                        results.insert(tx_digest, status);
                    }
                    let mut reserved_amounts = BTreeMap::new();
                    for withdraw in withdraws {
                        if results.get(&withdraw.tx_digest) == Some(&ScheduleStatus::SufficientFunds) {
                            for (account_id, reservation) in withdraw.reservations {
                                *reserved_amounts.entry(account_id).or_insert(0) += reservation;
                            }
                        }
                    }
                    schedule_results_tx.send(reserved_amounts).unwrap();
                }
                // Drop the sender so that the settlement task can exit.
                drop(schedule_results_tx);

                // Make sure all settlements are processed.
                settle_task.await.unwrap();

                results
            })
            .await
            .expect("Task timed out after 30 seconds");
        if let Some(expected_results) = &expected_results {
            assert_eq!(results.len(), expected_results.len());
            for (tx_digest, status) in results {
                assert_eq!(
                    &status,
                    expected_results.get(&tx_digest).unwrap(),
                    "Tx digest: {:?}",
                    tx_digest
                );
            }
        } else {
            expected_results = Some(results);
        }
    }
}
