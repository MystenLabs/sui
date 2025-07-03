// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_scheduler::balance_withdraw_scheduler::ScheduleResult;
use crate::execution_scheduler::balance_withdraw_scheduler::{
    balance_read::MockBalanceRead, scheduler::BalanceWithdrawScheduler, BalanceSettlement,
    ScheduleStatus, TxBalanceWithdraw,
};
use futures::stream::{FuturesUnordered, StreamExt};
use rand::{seq::SliceRandom, Rng};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
    transaction::Reservation,
};
use tokio::sync::oneshot;
use tokio::time::timeout;

#[derive(Clone)]
struct TestScheduler {
    mock_read: Arc<MockBalanceRead>,
    scheduler: Arc<BalanceWithdrawScheduler>,
}

impl TestScheduler {
    fn new(init_version: SequenceNumber, init_balances: BTreeMap<ObjectID, u64>) -> Self {
        let mock_read = Arc::new(MockBalanceRead::new(init_version, init_balances));
        let scheduler = BalanceWithdrawScheduler::new(mock_read.clone(), init_version);
        Self {
            mock_read,
            scheduler,
        }
    }

    fn settle_balance_changes(&self, version: SequenceNumber, changes: BTreeMap<ObjectID, i128>) {
        self.mock_read
            .settle_balance_changes(version, changes.clone());
        self.scheduler.settle_balances(BalanceSettlement {
            accumulator_version: version,
            balance_changes: changes,
        });
    }
}

async fn wait_for_results(
    mut receivers: FuturesUnordered<oneshot::Receiver<ScheduleResult>>,
    expected_results: BTreeMap<TransactionDigest, ScheduleStatus>,
) {
    timeout(Duration::from_secs(3), async {
        let mut results = BTreeMap::new();
        while let Some(result) = receivers.next().await {
            let result = result.unwrap();
            results.insert(result.tx_digest, result.status);
        }
        assert_eq!(results, expected_results);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_basic_sufficient_balance() {
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));

    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(50))]),
    };

    let receivers = test
        .scheduler
        .schedule_withdraws(init_version, vec![withdraw.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;
}

#[tokio::test]
async fn test_basic_insufficient_balance() {
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));

    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(150))]),
    };

    let receivers = test
        .scheduler
        .schedule_withdraws(init_version, vec![withdraw.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::InsufficientBalance)]),
    )
    .await;
}

#[tokio::test]
async fn test_already_scheduled() {
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));

    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(50))]),
    };

    let receivers = test
        .scheduler
        .schedule_withdraws(init_version, vec![withdraw.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;

    let receivers = test
        .scheduler
        .schedule_withdraws(init_version, vec![withdraw.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::AlreadyScheduled)]),
    )
    .await;
}

#[tokio::test]
async fn test_basic_settlement() {
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));

    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(50))]),
    };

    let receivers = test
        .scheduler
        .schedule_withdraws(init_version, vec![withdraw.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;

    let next_version = init_version.next();
    test.settle_balance_changes(next_version, BTreeMap::from([(account, -50i128)]));

    let receivers = test
        .scheduler
        .schedule_withdraws(next_version, vec![withdraw.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;
}

#[tokio::test]
async fn test_out_of_order_settlements() {
    let v0 = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(v0, BTreeMap::from([(account, 100)]));

    let v1 = v0.next();
    let v2 = v1.next();

    test.settle_balance_changes(v2, BTreeMap::from([(account, -80i128)]));
    test.settle_balance_changes(v1, BTreeMap::from([(account, -20i128)]));

    let withdraw1 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(50))]),
    };
    let receivers = test
        .scheduler
        .schedule_withdraws(v0, vec![withdraw1.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw1.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;

    let withdraw2 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(80))]),
    };
    let receivers = test
        .scheduler
        .schedule_withdraws(v1, vec![withdraw2.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw2.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;
}

#[tokio::test]
async fn test_multi_accounts() {
    let init_version = SequenceNumber::from_u64(0);
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let test = TestScheduler::new(
        init_version,
        BTreeMap::from([(account1, 100), (account2, 100)]),
    );

    let reservations1 = BTreeMap::from([
        (account1, Reservation::MaxAmountU64(50)),
        (account2, Reservation::MaxAmountU64(50)),
    ]);
    let withdraw1 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: reservations1,
    };
    let reservations2 = BTreeMap::from([
        (account1, Reservation::MaxAmountU64(50)),
        (account2, Reservation::MaxAmountU64(60)),
    ]);
    let withdraw2 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: reservations2,
    };
    let reservations3 = BTreeMap::from([
        (account1, Reservation::MaxAmountU64(50)),
        (account2, Reservation::MaxAmountU64(50)),
    ]);
    let withdraw3 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: reservations3,
    };

    let receivers = test.scheduler.schedule_withdraws(
        init_version,
        vec![withdraw1.clone(), withdraw2.clone(), withdraw3.clone()],
    );
    wait_for_results(
        receivers,
        BTreeMap::from([
            (withdraw1.tx_digest, ScheduleStatus::SufficientBalance),
            (withdraw2.tx_digest, ScheduleStatus::InsufficientBalance),
            (withdraw3.tx_digest, ScheduleStatus::SufficientBalance),
        ]),
    )
    .await;
}

#[tokio::test]
async fn test_multi_settlements() {
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));

    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(50))]),
    };

    let receivers = test
        .scheduler
        .schedule_withdraws(init_version, vec![withdraw.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;

    let next_version = init_version.next();
    test.settle_balance_changes(next_version, BTreeMap::from([(account, -50i128)]));

    let receivers = test
        .scheduler
        .schedule_withdraws(next_version, vec![withdraw.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;

    let next_version = next_version.next();
    test.settle_balance_changes(next_version, BTreeMap::from([(account, -50i128)]));

    let receivers = test
        .scheduler
        .schedule_withdraws(next_version, vec![withdraw.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::InsufficientBalance)]),
    )
    .await;
}

#[tokio::test]
async fn test_settlement_far_ahead_of_schedule() {
    let v0 = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(v0, BTreeMap::from([(account, 100)]));
    let v1 = v0.next();
    let v2 = v1.next();
    let v3 = v2.next();

    // From v0 to v1, we reserve 100, but does not withdraw anything.
    test.settle_balance_changes(v1, BTreeMap::from([]));

    // From v1 to v2, we reserve 100, and withdraw 50.
    test.settle_balance_changes(v2, BTreeMap::from([(account, -50i128)]));

    // From v2 to v3, we reserve 50, and withdraw 50.
    test.settle_balance_changes(v3, BTreeMap::from([(account, -50i128)]));

    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(100))]),
    };
    let receivers = test
        .scheduler
        .schedule_withdraws(v0, vec![withdraw.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;

    let receivers = test
        .scheduler
        .schedule_withdraws(v1, vec![withdraw.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;

    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(50))]),
    };

    let receivers = test
        .scheduler
        .schedule_withdraws(v2, vec![withdraw.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;
}

#[tokio::test]
async fn test_withdraw_entire_balance() {
    let init_version = SequenceNumber::from_u64(0);
    let next_version = init_version.next();
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));

    let withdraws1 = vec![
        TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::EntireBalance)]),
        },
        TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(50))]),
        },
        TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::EntireBalance)]),
        },
    ];

    let receivers1 = test
        .scheduler
        .schedule_withdraws(init_version, withdraws1.clone());

    let withdraws2 = vec![
        TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(100))]),
        },
        TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::EntireBalance)]),
        },
        TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(1))]),
        },
    ];

    let receivers2 = test
        .scheduler
        .schedule_withdraws(next_version, withdraws2.clone());

    test.settle_balance_changes(next_version, BTreeMap::new());

    wait_for_results(
        receivers1,
        BTreeMap::from([
            (withdraws1[0].tx_digest, ScheduleStatus::SufficientBalance),
            (withdraws1[1].tx_digest, ScheduleStatus::InsufficientBalance),
            (withdraws1[2].tx_digest, ScheduleStatus::InsufficientBalance),
        ]),
    )
    .await;

    wait_for_results(
        receivers2,
        BTreeMap::from([
            (withdraws2[0].tx_digest, ScheduleStatus::SufficientBalance),
            (withdraws2[1].tx_digest, ScheduleStatus::InsufficientBalance),
            (withdraws2[2].tx_digest, ScheduleStatus::InsufficientBalance),
        ]),
    )
    .await;
}

#[tokio::test]
async fn stress_test() {
    let num_accounts = 5;
    let num_transactions = 10000;

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

    let mut withdraws = Vec::new();
    let mut settlements = Vec::new();
    let mut cur_reservations = Vec::new();

    for idx in 0..num_transactions {
        let num_accounts = rng.gen_range(1..3);
        let account_ids = accounts
            .choose_multiple(&mut rng, num_accounts)
            .cloned()
            .collect::<Vec<_>>();
        let reservations = account_ids
            .iter()
            .map(|account_id| {
                (
                    *account_id,
                    if rng.gen_bool(0.8) {
                        Reservation::MaxAmountU64(rng.gen_range(1..10))
                    } else {
                        Reservation::EntireBalance
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        cur_reservations.push(TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations,
        });
        if rng.gen_bool(0.2) || idx == num_transactions - 1 {
            // Every now and then we generate a settlement to advance the version.
            // We don't really settle any balance changes here, as this test
            // is primarily focusing on the scheduler's ability to handle
            // random combinations ofwithdraws reservations.
            withdraws.push((version, std::mem::take(&mut cur_reservations)));
            version = version.next();
            settlements.push((version, BTreeMap::new()));
        }
    }

    // Run through the scheduler many times and check that the results are always the same.
    let mut expected_results = None;
    let mut handles = Vec::new();

    // Spawn 10 concurrent tasks
    for _ in 0..10 {
        let init_balances = init_balances.clone();
        let settlements = settlements.clone();
        let withdraws = withdraws.clone();

        let handle = tokio::spawn(async move {
            let test = TestScheduler::new(version, init_balances);

            // Start a separate thread to run all settlements on the scheduler.
            let test_clone = test.clone();
            let settlements = settlements.clone();
            let settle_task = tokio::spawn(async move {
                for (version, balance_changes) in settlements {
                    test_clone.settle_balance_changes(version, balance_changes);
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            });

            let mut all_receivers = FuturesUnordered::new();
            for (version, withdraws) in withdraws {
                let receivers = test.scheduler.schedule_withdraws(version, withdraws);
                tokio::time::sleep(Duration::from_millis(5)).await;
                all_receivers.extend(receivers);
            }
            // Wait for the settle task to finish.
            settle_task.await.unwrap();

            let mut results = BTreeMap::new();
            while let Some(result) = all_receivers.next().await {
                let result = result.unwrap();
                results.insert(result.tx_digest, result.status);
            }
            results
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete and compare results
    for handle in handles {
        let results = handle.await.unwrap();
        if expected_results.is_none() {
            expected_results = Some(results);
        } else {
            assert_eq!(expected_results, Some(results));
        }
    }
}
