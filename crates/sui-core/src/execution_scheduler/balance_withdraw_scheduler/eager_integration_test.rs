// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod eager_integration_tests {
    use crate::execution_scheduler::balance_withdraw_scheduler::{
        balance_read::MockBalanceRead, scheduler::BalanceWithdrawScheduler, BalanceSettlement,
        ScheduleStatus, TxBalanceWithdraw,
    };
    use futures::stream::StreamExt;
    use std::{collections::BTreeMap, sync::Arc};
    use sui_types::{
        base_types::{ObjectID, SequenceNumber},
        digests::TransactionDigest,
        transaction::Reservation,
    };

    #[tokio::test]
    async fn test_eager_scheduler_basic_functionality() {
        let init_version = SequenceNumber::from_u64(0);
        let account = ObjectID::random();
        let init_balances = BTreeMap::from([(account, 1000)]);

        let mock_read = Arc::new(MockBalanceRead::new(init_version, init_balances));
        let scheduler = BalanceWithdrawScheduler::new_eager(mock_read.clone(), init_version);

        // Test eager scheduling by submitting multiple transactions in one batch
        let withdraw1 = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(500))]),
        };

        let withdraw2 = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(400))]),
        };

        let withdraw3 = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(200))]),
        };

        // Schedule all three at once - eager scheduler should handle them sequentially
        let mut receivers = scheduler.schedule_withdraws(
            init_version,
            vec![withdraw1.clone(), withdraw2.clone(), withdraw3.clone()],
        );

        // First should succeed (500 out of 1000)
        let result1 = receivers.next().await.unwrap().unwrap();
        assert_eq!(result1.status, ScheduleStatus::SufficientBalance);
        assert_eq!(result1.tx_digest, withdraw1.tx_digest);

        // Second should succeed (400 out of remaining 500)
        let result2 = receivers.next().await.unwrap().unwrap();
        assert_eq!(result2.status, ScheduleStatus::SufficientBalance);
        assert_eq!(result2.tx_digest, withdraw2.tx_digest);

        // Third should fail (200 exceeds remaining 100)
        let result3 = receivers.next().await.unwrap().unwrap();
        assert_eq!(result3.status, ScheduleStatus::InsufficientBalance);
        assert_eq!(result3.tx_digest, withdraw3.tx_digest);

        // Test 4: After settlement, reservations are reset
        let next_version = init_version.next();
        mock_read.settle_balance_changes(next_version, BTreeMap::from([(account, -900i128)]));
        scheduler.settle_balances(BalanceSettlement {
            accumulator_version: next_version,
            balance_changes: BTreeMap::from([(account, -900i128)]),
        });

        // Now only 100 balance remains, this should succeed
        let withdraw4 = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(100))]),
        };

        let mut receivers = scheduler.schedule_withdraws(next_version, vec![withdraw4.clone()]);
        let result = receivers.next().await.unwrap().unwrap();
        assert_eq!(result.status, ScheduleStatus::SufficientBalance);
    }

    #[tokio::test]
    async fn test_eager_scheduler_entire_balance() {
        let init_version = SequenceNumber::from_u64(0);
        let account = ObjectID::random();
        let init_balances = BTreeMap::from([(account, 1000)]);

        let mock_read = Arc::new(MockBalanceRead::new(init_version, init_balances));
        let scheduler = BalanceWithdrawScheduler::new_eager(mock_read.clone(), init_version);

        // Test EntireBalance reservation with multiple transactions in one batch
        let withdraw1 = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::EntireBalance)]),
        };

        let withdraw2 = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(1))]),
        };

        // Schedule both at once - first should succeed, second should fail
        let mut receivers =
            scheduler.schedule_withdraws(init_version, vec![withdraw1.clone(), withdraw2.clone()]);

        let result1 = receivers.next().await.unwrap().unwrap();
        assert_eq!(result1.status, ScheduleStatus::SufficientBalance);
        assert_eq!(result1.tx_digest, withdraw1.tx_digest);

        let result2 = receivers.next().await.unwrap().unwrap();
        assert_eq!(result2.status, ScheduleStatus::InsufficientBalance);
        assert_eq!(result2.tx_digest, withdraw2.tx_digest);
    }

    #[tokio::test]
    async fn test_eager_scheduler_already_scheduled() {
        let init_version = SequenceNumber::from_u64(0);
        let account = ObjectID::random();
        let init_balances = BTreeMap::from([(account, 1000)]);

        let mock_read = Arc::new(MockBalanceRead::new(init_version, init_balances));
        let scheduler = BalanceWithdrawScheduler::new_eager(mock_read.clone(), init_version);

        let withdraw = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(500))]),
        };

        // First scheduling should succeed
        let mut receivers = scheduler.schedule_withdraws(init_version, vec![withdraw.clone()]);
        let result = receivers.next().await.unwrap().unwrap();
        assert_eq!(result.status, ScheduleStatus::SufficientBalance);

        // Second scheduling with same version should return AlreadyExecuted
        let mut receivers = scheduler.schedule_withdraws(init_version, vec![withdraw.clone()]);
        let result = receivers.next().await.unwrap().unwrap();
        assert_eq!(result.status, ScheduleStatus::AlreadyExecuted);
    }
}
