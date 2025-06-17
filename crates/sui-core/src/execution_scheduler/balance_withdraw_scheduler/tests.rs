// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_scheduler::balance_withdraw_scheduler::{
    balance_read::MockBalanceRead, scheduler::BalanceWithdrawScheduler, BalanceSettlement,
    ScheduleResult, TxBalanceWithdraw,
};
use rand::{seq::SliceRandom, Rng};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
};
#[cfg(test)]
use tokio::sync::oneshot;

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

#[cfg(test)]
async fn wait_until(receiver: oneshot::Receiver<ScheduleResult>, until: ScheduleResult) {
    use std::time::Duration;

    use tokio::time::timeout;

    timeout(Duration::from_secs(3), async {
        assert_eq!(receiver.await.unwrap(), until);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_basic_sufficient_balance() {
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));

    let reservations = BTreeMap::from([(account, 50)]);
    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations,
    };

    let receivers = test
        .scheduler
        .schedule_withdraws(init_version, vec![withdraw]);
    for (_, receiver) in receivers {
        wait_until(receiver, ScheduleResult::SufficientBalance).await;
    }
}

#[tokio::test]
async fn test_basic_insufficient_balance() {
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));

    let reservations = BTreeMap::from([(account, 150)]);
    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations,
    };

    let receivers = test
        .scheduler
        .schedule_withdraws(init_version, vec![withdraw]);
    for (_, receiver) in receivers {
        wait_until(receiver, ScheduleResult::InsufficientBalance).await;
    }
}

#[tokio::test]
async fn test_already_scheduled() {
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));

    let reservations = BTreeMap::from([(account, 50)]);
    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations,
    };

    let receivers = test
        .scheduler
        .schedule_withdraws(init_version, vec![withdraw.clone()]);
    for (_, receiver) in receivers {
        wait_until(receiver, ScheduleResult::SufficientBalance).await;
    }

    let receivers = test
        .scheduler
        .schedule_withdraws(init_version, vec![withdraw]);
    for (_, receiver) in receivers {
        wait_until(receiver, ScheduleResult::AlreadyScheduled).await;
    }
}

#[tokio::test]
async fn test_basic_settlement() {
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));

    let reservations = BTreeMap::from([(account, 50)]);
    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations,
    };

    let receivers = test
        .scheduler
        .schedule_withdraws(init_version, vec![withdraw.clone()]);
    for (_, receiver) in receivers {
        wait_until(receiver, ScheduleResult::SufficientBalance).await;
    }

    let next_version = init_version.next();
    test.settle_balance_changes(next_version, BTreeMap::from([(account, -50i128)]));

    let receivers = test
        .scheduler
        .schedule_withdraws(next_version, vec![withdraw]);
    for (_, receiver) in receivers {
        wait_until(receiver, ScheduleResult::SufficientBalance).await;
    }
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
        reservations: BTreeMap::from([(account, 50)]),
    };
    let mut receivers = test
        .scheduler
        .schedule_withdraws(v0, vec![withdraw1.clone()]);
    wait_until(
        receivers.remove(&withdraw1.tx_digest).unwrap(),
        ScheduleResult::SufficientBalance,
    )
    .await;

    let withdraw2 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, 80)]),
    };
    let mut receivers = test
        .scheduler
        .schedule_withdraws(v1, vec![withdraw2.clone()]);
    wait_until(
        receivers.remove(&withdraw2.tx_digest).unwrap(),
        ScheduleResult::SufficientBalance,
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

    let reservations1 = BTreeMap::from([(account1, 50), (account2, 50)]);
    let withdraw1 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: reservations1,
    };
    let reservations2 = BTreeMap::from([(account1, 50), (account2, 60)]);
    let withdraw2 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: reservations2,
    };
    let reservations3 = BTreeMap::from([(account1, 50), (account2, 50)]);
    let withdraw3 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: reservations3,
    };

    let mut receivers = test.scheduler.schedule_withdraws(
        init_version,
        vec![withdraw1.clone(), withdraw2.clone(), withdraw3.clone()],
    );
    wait_until(
        receivers.remove(&withdraw1.tx_digest).unwrap(),
        ScheduleResult::SufficientBalance,
    )
    .await;
    wait_until(
        receivers.remove(&withdraw2.tx_digest).unwrap(),
        ScheduleResult::InsufficientBalance,
    )
    .await;
    wait_until(
        receivers.remove(&withdraw3.tx_digest).unwrap(),
        ScheduleResult::SufficientBalance,
    )
    .await;
}

#[tokio::test]
async fn test_multi_settlements() {
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));

    let reservations = BTreeMap::from([(account, 50)]);
    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations,
    };

    let mut receivers = test
        .scheduler
        .schedule_withdraws(init_version, vec![withdraw.clone()]);
    wait_until(
        receivers.remove(&withdraw.tx_digest).unwrap(),
        ScheduleResult::SufficientBalance,
    )
    .await;

    let next_version = init_version.next();
    test.settle_balance_changes(next_version, BTreeMap::from([(account, -50i128)]));

    let mut receivers = test
        .scheduler
        .schedule_withdraws(next_version, vec![withdraw.clone()]);
    wait_until(
        receivers.remove(&withdraw.tx_digest).unwrap(),
        ScheduleResult::SufficientBalance,
    )
    .await;

    let next_version = next_version.next();
    test.settle_balance_changes(next_version, BTreeMap::from([(account, -50i128)]));

    let mut receivers = test
        .scheduler
        .schedule_withdraws(next_version, vec![withdraw.clone()]);
    wait_until(
        receivers.remove(&withdraw.tx_digest).unwrap(),
        ScheduleResult::InsufficientBalance,
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
        reservations: BTreeMap::from([(account, 100)]),
    };
    let mut receivers = test
        .scheduler
        .schedule_withdraws(v0, vec![withdraw.clone()]);
    wait_until(
        receivers.remove(&withdraw.tx_digest).unwrap(),
        ScheduleResult::SufficientBalance,
    )
    .await;

    let mut receivers = test
        .scheduler
        .schedule_withdraws(v1, vec![withdraw.clone()]);
    wait_until(
        receivers.remove(&withdraw.tx_digest).unwrap(),
        ScheduleResult::SufficientBalance,
    )
    .await;

    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account, 50)]),
    };

    let mut receivers = test
        .scheduler
        .schedule_withdraws(v2, vec![withdraw.clone()]);
    wait_until(
        receivers.remove(&withdraw.tx_digest).unwrap(),
        ScheduleResult::SufficientBalance,
    )
    .await;
}

#[tokio::test]
async fn stress_test() {
    let num_accounts = 100;
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
                Some((*account_id, rng.gen_range(0..10)))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();
    let test = TestScheduler::new(version, init_balances.clone());

    let mut withdraws = Vec::new();
    let mut expected_results = BTreeMap::new();
    let mut settlements = Vec::new();
    let mut balances = init_balances;
    let mut cur_reservations = Vec::new();

    for idx in 0..num_transactions {
        let num_accounts = rng.gen_range(1..3);
        let account_ids = accounts
            .choose_multiple(&mut rng, num_accounts)
            .cloned()
            .collect::<Vec<_>>();
        let reservations = account_ids
            .iter()
            .map(|account_id| (*account_id, rng.gen_range(0..10)))
            .collect::<BTreeMap<_, _>>();
        cur_reservations.push(TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations,
        });
        if rng.gen_bool(0.2) || idx == num_transactions - 1 {
            let mut accumulated_reservations: BTreeMap<ObjectID, u64> = BTreeMap::new();
            let mut balance_changes: BTreeMap<ObjectID, i128> = BTreeMap::new();
            for reservation in &cur_reservations {
                let mut success = true;
                for (account_id, amount) in &reservation.reservations {
                    if *amount
                        + accumulated_reservations
                            .get(account_id)
                            .copied()
                            .unwrap_or_default()
                        > balances.get(account_id).copied().unwrap_or_default()
                    {
                        success = false;
                        break;
                    }
                }
                if success {
                    for (account_id, amount) in &reservation.reservations {
                        *accumulated_reservations.entry(*account_id).or_default() += *amount;
                    }
                    expected_results
                        .insert(reservation.tx_digest, ScheduleResult::SufficientBalance);
                    for (account_id, amount) in &reservation.reservations {
                        *balance_changes.entry(*account_id).or_default() +=
                            -(rng.gen_range(0..=*amount) as i128);
                    }
                } else {
                    expected_results
                        .insert(reservation.tx_digest, ScheduleResult::InsufficientBalance);
                }
            }
            let num_deposits = rng.gen_range(0..5);
            for _ in 0..num_deposits {
                let account_id = accounts.choose(&mut rng).unwrap();
                let amount = rng.gen_range(0..10) as i128;
                *balance_changes.entry(*account_id).or_default() += amount;
            }
            for (account_id, amount) in &balance_changes {
                let existing = balances.entry(*account_id).or_default();
                *existing = (*existing as i128 + *amount) as u64;
            }
            withdraws.push((version, std::mem::take(&mut cur_reservations)));
            version = version.next();
            settlements.push((version, balance_changes));
        }
    }

    // Start a separate thread to run all settlements on the scheduler.
    let test_clone = test.clone();
    let settle_task = tokio::spawn(async move {
        for (version, balance_changes) in settlements {
            test_clone.settle_balance_changes(version, balance_changes);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });

    // Run all withdraws on the scheduler.
    let mut all_receivers = BTreeMap::new();
    for (version, withdraws) in withdraws {
        let receivers = test.scheduler.schedule_withdraws(version, withdraws);
        tokio::time::sleep(Duration::from_millis(5)).await;
        all_receivers.extend(receivers);
    }

    // Wait for the settle task to finish.
    settle_task.await.unwrap();

    // Wait for all receivers to be processed.
    for (tx_digest, receiver) in all_receivers {
        wait_until(receiver, expected_results[&tx_digest]).await;
    }
}
