// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_scheduler::balance_withdraw_scheduler::ScheduleResult;
use crate::execution_scheduler::balance_withdraw_scheduler::{
    balance_read::MockBalanceRead, scheduler::BalanceWithdrawScheduler, BalanceSettlement,
    ScheduleStatus, TxBalanceWithdraw,
};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use mysten_metrics::monitored_mpsc::unbounded_channel;
use parking_lot::Mutex;
use rand::{seq::SliceRandom, Rng};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use sui_macros::sim_test;
use sui_types::{
    accumulator_root::AccumulatorObjId,
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
};
use tokio::sync::oneshot;
use tokio::time::timeout;
use tracing::debug;

/// A test scheduler that runs multiple schedulers in parallel and waits for all results to be received.
/// It internally checks that all schedulers return the same results.
#[derive(Clone)]
struct TestScheduler {
    mock_read: Arc<MockBalanceRead>,
    schedulers: BTreeMap<String, BalanceWithdrawScheduler>,
}

impl TestScheduler {
    fn new(init_version: SequenceNumber, init_balances: BTreeMap<ObjectID, u128>) -> Self {
        let mock_read = Arc::new(MockBalanceRead::new(init_version, init_balances));
        let naive_scheduler = BalanceWithdrawScheduler::new(mock_read.clone(), init_version);
        Self {
            mock_read,
            schedulers: BTreeMap::from([("naive_scheduler".to_string(), naive_scheduler)]),
        }
    }

    /// Spawns a task to collect results from all schedulers, check that their results match,
    /// and return a unified list of receivers to the caller.
    fn schedule_withdraws(
        &self,
        version: SequenceNumber,
        withdraws: Vec<TxBalanceWithdraw>,
    ) -> FuturesUnordered<oneshot::Receiver<ScheduleResult>> {
        let (forward_senders, unified_receivers): (BTreeMap<_, _>, FuturesUnordered<_>) = withdraws
            .iter()
            .map(|withdraw| {
                let (sender, receiver) = oneshot::channel();
                ((withdraw.tx_digest, sender), receiver)
            })
            .unzip();
        // Note that we must call schedule_withdraws async outside the spawn task,
        // since the system expects the schedule_withdraws call to be in order.
        let all_receivers = self
            .schedulers
            .iter()
            .map(|(name, scheduler)| {
                let receivers = scheduler.schedule_withdraws(version, withdraws.clone());
                (name.clone(), receivers)
            })
            .collect::<BTreeMap<_, _>>();
        tokio::spawn(async move {
            let mut unique_results = None;
            for (name, receivers) in all_receivers {
                let mut local_results = BTreeMap::new();
                for receiver in receivers {
                    let result = receiver.await.unwrap();
                    local_results.insert(result.tx_digest, result);
                }
                if let Some(results) = &unique_results {
                    assert_eq!(results, &local_results, "Scheduler: {:?}", name);
                } else {
                    unique_results = Some(local_results);
                }
            }
            let mut unique_results = unique_results.unwrap();
            for (tx_digest, sender) in forward_senders {
                let result = unique_results.remove(&tx_digest).unwrap();
                let _ = sender.send(result);
            }
        });
        unified_receivers
    }

    /// Settles the balance changes for all schedulers.
    fn settle_balance_changes(&self, changes: BTreeMap<ObjectID, i128>) {
        let accumulator_changes = changes
            .iter()
            .map(|(id, value)| (AccumulatorObjId::new_unchecked(*id), *value))
            .collect();
        self.mock_read.settle_balance_changes(accumulator_changes);
        self.schedulers.values().for_each(|scheduler| {
            let accumulator_changes = changes
                .iter()
                .map(|(id, value)| (AccumulatorObjId::new_unchecked(*id), *value))
                .collect();
            scheduler.settle_balances(BalanceSettlement {
                balance_changes: accumulator_changes,
            });
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
    .unwrap()
}

#[tokio::test]
#[should_panic(expected = "Elapsed")]
async fn test_schedule_wait_for_settlement() {
    // This test checks that a withdraw cannot be scheduled until
    // a settlement, and if there is no settlement we would lose liveness.
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));

    let withdraw = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 200)]),
    };

    let receivers = test.schedule_withdraws(init_version.next(), vec![withdraw.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;
}

#[tokio::test]
async fn test_schedules_and_settles() {
    let v0 = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(v0, BTreeMap::from([(account, 100)]));

    let withdraw0 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 60)]),
    };
    let withdraw1 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 60)]),
    };
    let withdraw2 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 60)]),
    };

    let receivers = test.schedule_withdraws(v0, vec![withdraw0.clone()]);

    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw0.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;

    let v1 = v0.next();
    let receivers = test.schedule_withdraws(v1, vec![withdraw1.clone()]);

    // 100 -> 40, v0 -> v1
    test.settle_balance_changes(BTreeMap::from([(account, -60)]));

    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw1.tx_digest, ScheduleStatus::InsufficientBalance)]),
    )
    .await;

    let v2 = v1.next();
    let receivers = test.schedule_withdraws(v2, vec![withdraw2.clone()]);

    // 40 -> 60, v1 -> v2
    test.settle_balance_changes(BTreeMap::from([(account, 20)]));

    wait_for_results(
        receivers,
        BTreeMap::from([(withdraw2.tx_digest, ScheduleStatus::SufficientBalance)]),
    )
    .await;
}

#[tokio::test]
async fn test_already_executed() {
    let init_version = SequenceNumber::from_u64(0);
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let test = TestScheduler::new(
        init_version,
        BTreeMap::from([(account1, 100), (account2, 200)]),
    );

    // Advance the accumulator version
    test.settle_balance_changes(BTreeMap::new());

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Try to schedule multiple withdraws for the old version
    let withdraw1 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 50)]),
    };
    let withdraw2 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account2), 100)]),
    };

    let receivers =
        test.schedule_withdraws(init_version, vec![withdraw1.clone(), withdraw2.clone()]);
    wait_for_results(
        receivers,
        BTreeMap::from([
            (withdraw1.tx_digest, ScheduleStatus::AlreadyExecuted),
            (withdraw2.tx_digest, ScheduleStatus::AlreadyExecuted),
        ]),
    )
    .await;
}

#[tokio::test]
async fn test_multiple_withdraws_same_version() {
    // This test checks that even though the second withdraw failed due to insufficient balance,
    // the third withdraw can still be scheduled since the second withdraw does not reserve any balance.
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 90)]));

    let withdraw1 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 50)]),
    };
    let withdraw2 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 50)]),
    };
    let withdraw3 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 40)]),
    };

    let receivers = test.schedule_withdraws(
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
async fn test_multiple_withdraws_multiple_accounts_same_version() {
    let init_version = SequenceNumber::from_u64(0);
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let test = TestScheduler::new(
        init_version,
        BTreeMap::from([(account1, 100), (account2, 100)]),
    );

    let withdraw1 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([
            (AccumulatorObjId::new_unchecked(account1), 100),
            (AccumulatorObjId::new_unchecked(account2), 200),
        ]),
    };
    let withdraw2 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 1)]),
    };
    let withdraw3 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account2), 100)]),
    };

    let receivers = test.schedule_withdraws(
        init_version,
        vec![withdraw1.clone(), withdraw2.clone(), withdraw3.clone()],
    );
    wait_for_results(
        receivers,
        BTreeMap::from([
            (withdraw1.tx_digest, ScheduleStatus::InsufficientBalance),
            (withdraw2.tx_digest, ScheduleStatus::InsufficientBalance),
            (withdraw3.tx_digest, ScheduleStatus::SufficientBalance),
        ]),
    )
    .await;
}

struct StressTestEnv {
    init_balances: BTreeMap<ObjectID, u128>,
    accounts: Vec<ObjectID>,
    withdraws: Vec<(SequenceNumber, Vec<TxBalanceWithdraw>)>,
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
            cur_reservations.push(TxBalanceWithdraw {
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

    let StressTestEnv {
        init_balances,
        accounts,
        withdraws,
    } = StressTestEnv::new(num_accounts, num_transactions);

    // Repeat the process many times to ensure deterministic results.
    let mut expected_results: Option<BTreeMap<TransactionDigest, ScheduleStatus>> = None;
    let settlements = Arc::new(Mutex::new(Vec::new()));
    for test_run in 0..50 {
        debug!("Running test instance {:?}", test_run);
        let init_balances = init_balances.clone();
        let accounts = accounts.clone();
        let withdraws = withdraws.clone();
        let settlements = settlements.clone();

        let results = tokio::time::timeout(
            Duration::from_secs(30),
            async {
                let test = TestScheduler::new(
                    SequenceNumber::from_u64(0),
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

                        test_clone.settle_balance_changes(settlements.lock()[idx].clone());
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
                    for result in receivers {
                        let result = result.await.unwrap();
                        debug!("Test instance received result for tx {:?} with status {:?} at version {:?}", result.tx_digest, result.status, version);
                        results.insert(result.tx_digest, result.status);
                    }
                    let mut reserved_amounts = BTreeMap::new();
                    for withdraw in withdraws {
                        if results.get(&withdraw.tx_digest) == Some(&ScheduleStatus::SufficientBalance) {
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
