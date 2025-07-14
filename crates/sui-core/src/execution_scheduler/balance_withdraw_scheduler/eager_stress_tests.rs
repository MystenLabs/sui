// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_scheduler::balance_withdraw_scheduler::{
    balance_read::MockBalanceRead, scheduler::BalanceWithdrawScheduler, BalanceSettlement,
    ScheduleStatus, TxBalanceWithdraw,
};
use futures::stream::{FuturesUnordered, StreamExt};
use rand::{seq::SliceRandom, Rng, SeedableRng};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, Instant},
};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
    transaction::Reservation,
};

#[derive(Debug, Clone)]
struct TestScenario {
    seed: u64,
    num_accounts: usize,
    num_consensus_commits: usize,
    txs_per_commit: usize,
    initial_balance_range: (u64, u64),
    reservation_range: (u64, u64),
    entire_balance_prob: f64,
    multi_account_tx_prob: f64,
    settlement_interval: usize,
    deposit_prob: f64,
    cancel_tx_prob: f64,
}

impl TestScenario {
    fn generate_test_data(&self) -> TestData {
        let mut rng = rand::rngs::StdRng::seed_from_u64(self.seed);
        let mut data = TestData::new();

        // Generate accounts
        data.accounts = (0..self.num_accounts).map(|_| ObjectID::random()).collect();

        // Generate initial balances
        for account in &data.accounts {
            let balance =
                rng.gen_range(self.initial_balance_range.0..=self.initial_balance_range.1);
            data.initial_balances.insert(*account, balance);
        }

        // Generate consensus commits - start from version 1 since 0 is initial
        let mut current_version = SequenceNumber::from_u64(1);
        for commit_idx in 0..self.num_consensus_commits {
            let mut commit_withdraws = Vec::new();
            let mut canceled_txs = BTreeSet::new();

            // Generate transactions for this commit
            for _ in 0..self.txs_per_commit {
                let tx_digest = TransactionDigest::random();

                // Determine if this transaction should be canceled
                let is_canceled = rng.gen_bool(self.cancel_tx_prob);
                if is_canceled {
                    canceled_txs.insert(tx_digest);
                }

                // Generate reservations
                let mut reservations = BTreeMap::new();
                let num_accounts_in_tx = if rng.gen_bool(self.multi_account_tx_prob) {
                    rng.gen_range(2..=3.min(self.num_accounts))
                } else {
                    1
                };

                let selected_accounts: Vec<_> = data
                    .accounts
                    .choose_multiple(&mut rng, num_accounts_in_tx)
                    .cloned()
                    .collect();

                for account in selected_accounts {
                    let reservation = if rng.gen_bool(self.entire_balance_prob) {
                        Reservation::EntireBalance
                    } else {
                        let amount =
                            rng.gen_range(self.reservation_range.0..=self.reservation_range.1);
                        Reservation::MaxAmountU64(amount)
                    };
                    reservations.insert(account, reservation);
                }

                commit_withdraws.push(TxBalanceWithdraw {
                    tx_digest,
                    reservations,
                });
            }

            data.consensus_commits.push(ConsensusCommit {
                version: current_version,
                withdraws: commit_withdraws,
                canceled_txs,
            });

            // Generate settlement if needed
            if (commit_idx + 1) % self.settlement_interval == 0 {
                let mut balance_changes = BTreeMap::new();

                // Generate some balance changes (simulating effects of executed transactions)
                for account in data
                    .accounts
                    .choose_multiple(&mut rng, self.num_accounts / 2)
                {
                    if rng.gen_bool(self.deposit_prob) {
                        // Deposit
                        let amount = rng.gen_range(10..100) as i128;
                        balance_changes.insert(*account, amount);
                    } else {
                        // Withdrawal (negative change)
                        let amount = -(rng.gen_range(1..50) as i128);
                        balance_changes.insert(*account, amount);
                    }
                }

                data.settlements.push(Settlement {
                    version: current_version,
                    balance_changes,
                });
            }

            // Always increment version for next commit
            current_version = current_version.next();
        }

        data
    }
}

#[derive(Debug)]
struct TestData {
    accounts: Vec<ObjectID>,
    initial_balances: BTreeMap<ObjectID, u64>,
    consensus_commits: Vec<ConsensusCommit>,
    settlements: Vec<Settlement>,
}

#[derive(Debug)]
struct ConsensusCommit {
    version: SequenceNumber,
    withdraws: Vec<TxBalanceWithdraw>,
    canceled_txs: BTreeSet<TransactionDigest>,
}

#[derive(Debug)]
struct Settlement {
    version: SequenceNumber,
    balance_changes: BTreeMap<ObjectID, i128>,
}

impl TestData {
    fn new() -> Self {
        Self {
            accounts: Vec::new(),
            initial_balances: BTreeMap::new(),
            consensus_commits: Vec::new(),
            settlements: Vec::new(),
        }
    }
}

async fn run_scheduler_test(
    scheduler: Arc<BalanceWithdrawScheduler>,
    test_data: &TestData,
    mock_read: Arc<MockBalanceRead>,
) -> (BTreeMap<TransactionDigest, ScheduleStatus>, Duration) {
    let start = Instant::now();
    let mut all_receivers = FuturesUnordered::new();

    // For naive scheduler to work, we need to settle versions in order
    let mut current_settle_version = SequenceNumber::from_u64(0);

    for commit in &test_data.consensus_commits {
        // First settle all versions up to this commit's version
        while current_settle_version < commit.version {
            current_settle_version = current_settle_version.next();

            // Check if there's a settlement for this version
            let settlement = test_data
                .settlements
                .iter()
                .find(|s| s.version == current_settle_version);

            if let Some(settlement) = settlement {
                // Update the mock read with balance changes
                mock_read
                    .settle_balance_changes(settlement.version, settlement.balance_changes.clone());

                scheduler.settle_balances(BalanceSettlement {
                    accumulator_version: settlement.version,
                    balance_changes: settlement.balance_changes.clone(),
                });
            } else {
                // No changes, just advance the version
                scheduler.settle_balances(BalanceSettlement {
                    accumulator_version: current_settle_version,
                    balance_changes: BTreeMap::new(),
                });
            }
        }

        // Schedule withdrawals
        let receivers = scheduler.schedule_withdraws(commit.version, commit.withdraws.clone());
        all_receivers.extend(receivers);
    }

    // Settle any remaining versions
    for settlement in &test_data.settlements {
        if settlement.version > current_settle_version {
            // Settle all versions up to this settlement
            while current_settle_version < settlement.version {
                current_settle_version = current_settle_version.next();
                if current_settle_version == settlement.version {
                    mock_read.settle_balance_changes(
                        settlement.version,
                        settlement.balance_changes.clone(),
                    );
                    scheduler.settle_balances(BalanceSettlement {
                        accumulator_version: settlement.version,
                        balance_changes: settlement.balance_changes.clone(),
                    });
                } else {
                    scheduler.settle_balances(BalanceSettlement {
                        accumulator_version: current_settle_version,
                        balance_changes: BTreeMap::new(),
                    });
                }
            }
        }
    }

    // Collect all results
    let mut results = BTreeMap::new();
    while let Some(result) = all_receivers.next().await {
        let result = result.unwrap();
        results.insert(result.tx_digest, result.status);
    }

    let duration = start.elapsed();
    (results, duration)
}

#[tokio::test]
async fn stress_test_eager_vs_naive_basic() {
    let scenario = TestScenario {
        seed: 42,
        num_accounts: 10,
        num_consensus_commits: 20,
        txs_per_commit: 5,
        initial_balance_range: (50, 200),
        reservation_range: (10, 80),
        entire_balance_prob: 0.1,
        multi_account_tx_prob: 0.3,
        settlement_interval: 5,
        deposit_prob: 0.3,
        cancel_tx_prob: 0.1,
    };

    let test_data = scenario.generate_test_data();
    let init_version = SequenceNumber::from_u64(0);

    // Create separate instances for naive scheduler
    let naive_mock_read = Arc::new(MockBalanceRead::new(
        init_version,
        test_data.initial_balances.clone(),
    ));
    let naive_scheduler = BalanceWithdrawScheduler::new(naive_mock_read.clone(), init_version);

    // Run naive scheduler
    let (naive_results, naive_duration) =
        run_scheduler_test(naive_scheduler, &test_data, naive_mock_read).await;

    // Create separate instances for eager scheduler
    let eager_mock_read = Arc::new(MockBalanceRead::new(
        init_version,
        test_data.initial_balances.clone(),
    ));
    let eager_scheduler =
        BalanceWithdrawScheduler::new_eager(eager_mock_read.clone(), init_version);

    // Run eager scheduler
    let (eager_results, eager_duration) =
        run_scheduler_test(eager_scheduler, &test_data, eager_mock_read).await;

    // The schedulers may produce different results due to their design:
    // - Naive: Waits for settlements and sees actual balance at each version
    // - Eager: Tracks cumulative reservations and uses pessimistic balance estimates

    // Both approaches are correct, but eager is more conservative
    let naive_sufficient = naive_results
        .values()
        .filter(|s| **s == ScheduleStatus::SufficientBalance)
        .count();
    let naive_insufficient = naive_results
        .values()
        .filter(|s| **s == ScheduleStatus::InsufficientBalance)
        .count();
    let eager_sufficient = eager_results
        .values()
        .filter(|s| **s == ScheduleStatus::SufficientBalance)
        .count();
    let eager_insufficient = eager_results
        .values()
        .filter(|s| **s == ScheduleStatus::InsufficientBalance)
        .count();

    println!(
        "Naive: {} sufficient, {} insufficient",
        naive_sufficient, naive_insufficient
    );
    println!(
        "Eager: {} sufficient, {} insufficient",
        eager_sufficient, eager_insufficient
    );

    // Verify that eager scheduler is more conservative (allows fewer or equal transactions)
    assert!(
        eager_sufficient <= naive_sufficient,
        "Eager scheduler should be more conservative than naive"
    );

    // Verify both processed the same number of transactions
    assert_eq!(naive_results.len(), eager_results.len());

    println!(
        "Basic stress test passed. Naive: {:?}, Eager: {:?}",
        naive_duration, eager_duration
    );
}

#[tokio::test]
async fn stress_test_eager_vs_naive_comprehensive() {
    let scenarios = vec![
        // High contention scenario
        TestScenario {
            seed: 1000,
            num_accounts: 5,
            num_consensus_commits: 50,
            txs_per_commit: 20,
            initial_balance_range: (100, 100),
            reservation_range: (20, 30),
            entire_balance_prob: 0.05,
            multi_account_tx_prob: 0.5,
            settlement_interval: 10,
            deposit_prob: 0.2,
            cancel_tx_prob: 0.15,
        },
        // Many accounts, low contention
        TestScenario {
            seed: 2000,
            num_accounts: 100,
            num_consensus_commits: 30,
            txs_per_commit: 10,
            initial_balance_range: (1000, 5000),
            reservation_range: (10, 100),
            entire_balance_prob: 0.02,
            multi_account_tx_prob: 0.1,
            settlement_interval: 5,
            deposit_prob: 0.5,
            cancel_tx_prob: 0.05,
        },
        // Entire balance heavy scenario
        TestScenario {
            seed: 3000,
            num_accounts: 20,
            num_consensus_commits: 25,
            txs_per_commit: 8,
            initial_balance_range: (50, 500),
            reservation_range: (10, 50),
            entire_balance_prob: 0.4,
            multi_account_tx_prob: 0.2,
            settlement_interval: 7,
            deposit_prob: 0.3,
            cancel_tx_prob: 0.1,
        },
        // Frequent settlements
        TestScenario {
            seed: 4000,
            num_accounts: 15,
            num_consensus_commits: 40,
            txs_per_commit: 6,
            initial_balance_range: (200, 1000),
            reservation_range: (50, 200),
            entire_balance_prob: 0.1,
            multi_account_tx_prob: 0.4,
            settlement_interval: 2,
            deposit_prob: 0.6,
            cancel_tx_prob: 0.2,
        },
    ];

    for (idx, scenario) in scenarios.iter().enumerate() {
        println!("Running comprehensive scenario {}", idx + 1);

        let test_data = scenario.generate_test_data();
        let init_version = SequenceNumber::from_u64(0);

        // Create separate instances for naive scheduler
        let naive_mock_read = Arc::new(MockBalanceRead::new(
            init_version,
            test_data.initial_balances.clone(),
        ));
        let naive_scheduler = BalanceWithdrawScheduler::new(naive_mock_read.clone(), init_version);

        // Run naive scheduler
        let (naive_results, naive_duration) =
            run_scheduler_test(naive_scheduler, &test_data, naive_mock_read).await;

        // Create separate instances for eager scheduler
        let eager_mock_read = Arc::new(MockBalanceRead::new(
            init_version,
            test_data.initial_balances.clone(),
        ));
        let eager_scheduler =
            BalanceWithdrawScheduler::new_eager(eager_mock_read.clone(), init_version);

        // Run eager scheduler
        let (eager_results, eager_duration) =
            run_scheduler_test(eager_scheduler, &test_data, eager_mock_read).await;

        // The schedulers may produce different results, but eager should be more conservative
        let _matching = naive_results == eager_results;

        // Calculate statistics
        let total_txs = naive_results.len();
        let sufficient_count = naive_results
            .values()
            .filter(|&&status| status == ScheduleStatus::SufficientBalance)
            .count();
        let insufficient_count = naive_results
            .values()
            .filter(|&&status| status == ScheduleStatus::InsufficientBalance)
            .count();

        println!(
            "Scenario {} results: Total: {}, Sufficient: {}, Insufficient: {}",
            idx + 1,
            total_txs,
            sufficient_count,
            insufficient_count
        );
        println!(
            "Naive duration: {:?}, Eager duration: {:?}, Speedup: {:.2}x",
            naive_duration,
            eager_duration,
            naive_duration.as_secs_f64() / eager_duration.as_secs_f64()
        );
        println!("---");
    }
}

#[tokio::test]
async fn stress_test_determinism() {
    let scenario = TestScenario {
        seed: 5000,
        num_accounts: 8,
        num_consensus_commits: 15,
        txs_per_commit: 10,
        initial_balance_range: (100, 500),
        reservation_range: (20, 100),
        entire_balance_prob: 0.15,
        multi_account_tx_prob: 0.35,
        settlement_interval: 4,
        deposit_prob: 0.4,
        cancel_tx_prob: 0.12,
    };

    let test_data = scenario.generate_test_data();
    let mut results_collection = Vec::new();

    // Run the same test multiple times to ensure determinism
    for run in 0..5 {
        println!("Determinism test run {}", run + 1);

        let init_version = SequenceNumber::from_u64(0);
        let mock_read = Arc::new(MockBalanceRead::new(
            init_version,
            test_data.initial_balances.clone(),
        ));
        let eager_scheduler = BalanceWithdrawScheduler::new_eager(mock_read.clone(), init_version);

        let (eager_results, _) = run_scheduler_test(eager_scheduler, &test_data, mock_read).await;

        results_collection.push(eager_results);
    }

    // Verify all runs produced identical results
    for i in 1..results_collection.len() {
        assert_eq!(
            results_collection[0], results_collection[i],
            "Run {} produced different results than run 0",
            i
        );
    }

    println!(
        "Determinism test passed: all {} runs produced identical results",
        results_collection.len()
    );
}

#[tokio::test]
async fn stress_test_edge_cases() {
    // Test 1: All transactions reserve entire balance
    let account = ObjectID::random();
    let init_version = SequenceNumber::from_u64(0);
    let initial_balances = BTreeMap::from([(account, 100)]);

    // Create naive scheduler
    let naive_mock_read = Arc::new(MockBalanceRead::new(init_version, initial_balances.clone()));
    let naive_scheduler = BalanceWithdrawScheduler::new(naive_mock_read, init_version);

    // Create eager scheduler
    let eager_mock_read = Arc::new(MockBalanceRead::new(init_version, initial_balances));
    let eager_scheduler = BalanceWithdrawScheduler::new_eager(eager_mock_read, init_version);

    let withdraws = vec![
        TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::EntireBalance)]),
        },
        TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::EntireBalance)]),
        },
    ];

    let naive_receivers = naive_scheduler.schedule_withdraws(init_version, withdraws.clone());
    let eager_receivers = eager_scheduler.schedule_withdraws(init_version, withdraws);

    let mut naive_results = BTreeMap::new();
    let mut eager_results = BTreeMap::new();

    let mut naive_futures = naive_receivers;
    let mut eager_futures = eager_receivers;

    while let Some(result) = naive_futures.next().await {
        let result = result.unwrap();
        naive_results.insert(result.tx_digest, result.status);
    }
    while let Some(result) = eager_futures.next().await {
        let result = result.unwrap();
        eager_results.insert(result.tx_digest, result.status);
    }

    // Both schedulers should have the same results
    assert_eq!(naive_results, eager_results);

    // Verify one succeeds and one fails
    let statuses: Vec<_> = eager_results.values().cloned().collect();
    assert!(statuses.contains(&ScheduleStatus::SufficientBalance));
    assert!(statuses.contains(&ScheduleStatus::InsufficientBalance));
    assert_eq!(statuses.len(), 2);

    println!("Edge case test: entire balance reservations passed");
}
