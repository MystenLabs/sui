// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::VecDeque, sync::Arc};

use sui_types::{accumulator_root::AccumulatorObjId, base_types::SequenceNumber};
use tracing::debug;

use crate::execution_scheduler::funds_withdraw_scheduler::address_funds::eager_scheduler::pending_withdraw::PendingWithdraw;

pub(crate) struct AccountState {
    account_id: AccumulatorObjId,
    /// Last known balance of the account (that is settled and persisted).
    last_updated_balance: u128,
    /// The version where last_updated_balance was read from.
    last_updated_version: SequenceNumber,
    reserved_funds: ReservedFunds,
    /// Withdraws that could not yet be scheduled due to insufficient funds, and
    /// hence have not reserved any funds yet. We track them so that we could schedule them
    /// anytime we may have sufficient funds.
    pending_reservations: VecDeque<Arc<PendingWithdraw>>,
}

#[derive(Default)]
struct ReservedFunds {
    /// The amount of funds that has been reserved for this account, for each accumulator version.
    /// This is tracked so that we could add them back to the account funds when we settle the withdraws.
    /// Since we always reserve in order of accumulator version, this vector is natually ordered, by
    /// SequenceNumber.
    reserved_funds: VecDeque<(SequenceNumber, u128)>,
    /// Sum of all amounts in reserved_funds, saved as an optimization to avoid summing it
    /// every time.
    total_reserved_funds_amount: u128,
}

impl AccountState {
    pub fn new(
        account_id: AccumulatorObjId,
        init_balance: u128,
        init_version: SequenceNumber,
    ) -> Self {
        debug!(
            "New account {:?} tracked at version {:?} with balance {:?}",
            account_id,
            init_version.value(),
            init_balance,
        );
        Self {
            account_id,
            last_updated_balance: init_balance,
            last_updated_version: init_version,
            reserved_funds: ReservedFunds::default(),
            pending_reservations: VecDeque::new(),
        }
    }

    pub fn try_reserve_new_withdraw(
        &mut self,
        new_withdraw: Arc<PendingWithdraw>,
        last_settled_version: SequenceNumber,
    ) -> bool {
        self.pending_reservations.push_back(new_withdraw);
        let len = self.pending_reservations.len();
        // If there are existing blocking withdraws, we cannot schedule this withdraw either.
        // Otherwise we schedule it immediately.
        if len > 1 {
            // The withdraws are scheduled in order of accumulator version, so the previous withdraw's version
            // must be less or equal to the current withdraw's version.
            assert!(
                self.pending_reservations[len - 2].accumulator_version()
                    <= self.pending_reservations[len - 1].accumulator_version()
            );
            false
        } else {
            self.try_reserve_front(last_settled_version)
        }
    }

    /// Try to process the first withdraw in the pending_reservations queue.
    /// Returns true if the processing was successful, i.e. we reached a deterministic
    /// decision on whether that withdraw can be satisfied for this account.
    fn try_reserve_front(&mut self, last_settled_version: SequenceNumber) -> bool {
        let Some(pending_withdraw) = self.pending_reservations.pop_front() else {
            return false;
        };
        assert!(
            pending_withdraw.accumulator_version() >= self.last_updated_version,
            "pending_withdraw.accumulator_version() = {:?}, self.last_updated_version = {:?}",
            pending_withdraw.accumulator_version(),
            self.last_updated_version
        );
        assert!(pending_withdraw.accumulator_version() >= last_settled_version);
        let to_reserve = pending_withdraw.pending_amount(&self.account_id);
        if self.reserved_funds.total_reserved_funds_amount() + to_reserve
            <= self.last_updated_balance
        {
            debug!(
                "Successfully reserved {:?} for account {:?} at version {:?}",
                to_reserve,
                self.account_id,
                pending_withdraw.accumulator_version().value(),
            );
            self.reserved_funds
                .add_reserved_fund(pending_withdraw.accumulator_version(), to_reserve);
            pending_withdraw.remove_pending_account(&self.account_id);
            return true;
        }
        if pending_withdraw.accumulator_version() == last_settled_version {
            pending_withdraw.notify_insufficient_funds();
            return true;
        }
        // Failed to reserve, put the pending withdraw back to the front of the queue.
        self.pending_reservations.push_front(pending_withdraw);
        false
    }

    pub fn settle_funds(&mut self, settled: i128, next_version: SequenceNumber) {
        debug!(
            "Settling funds for account {:?} with amount {:?} at version {:?}",
            self.account_id,
            settled,
            next_version.value(),
        );
        // If the next_version is less or equal to the last_updated_version,
        // it means the state tracked for this account is already up to date.
        // There is no need to update its balance.
        if next_version > self.last_updated_version {
            let new_balance = (self.last_updated_balance as i128)
                .checked_add(settled)
                .unwrap();
            assert!(new_balance >= 0);
            self.last_updated_balance = new_balance as u128;
            self.last_updated_version = next_version;
        }
        self.reserved_funds.settle_fund(next_version);
        while self.try_reserve_front(next_version) {}
    }

    pub fn is_empty(&self) -> bool {
        self.pending_reservations.is_empty() && self.reserved_funds.reserved_funds.is_empty()
    }
}

impl ReservedFunds {
    fn total_reserved_funds_amount(&self) -> u128 {
        self.total_reserved_funds_amount
    }

    fn add_reserved_fund(&mut self, accumulator_version: SequenceNumber, amount: u128) {
        self.total_reserved_funds_amount += amount;

        if let Some(entry) = self.reserved_funds.back_mut() {
            if entry.0 == accumulator_version {
                entry.1 = entry.1.checked_add(amount).unwrap();
                return;
            }
            // Reservations are processed in order, so we must never see the version
            // decrease.
            assert!(entry.0 < accumulator_version);
        }
        self.reserved_funds.push_back((accumulator_version, amount));
    }

    fn settle_fund(&mut self, next_version: SequenceNumber) {
        if let Some(entry) = self.reserved_funds.front().copied() {
            if entry.0.next() == next_version {
                self.reserved_funds.pop_front();
                self.total_reserved_funds_amount = self
                    .total_reserved_funds_amount
                    .checked_sub(entry.1)
                    .unwrap();
            } else {
                // If there exists reserved funds at even earlier versions, they must have
                // already been settled in the past.
                assert!(entry.0 >= next_version);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use sui_types::{
        accumulator_root::AccumulatorObjId, base_types::ObjectID, digests::TransactionDigest,
    };
    use tokio::sync::oneshot;

    use crate::execution_scheduler::funds_withdraw_scheduler::TxFundsWithdraw;

    fn make_account_id(byte: u8) -> AccumulatorObjId {
        AccumulatorObjId::new_unchecked(ObjectID::from_single_byte(byte))
    }

    fn make_pending_withdraw(
        account_id: AccumulatorObjId,
        amount: u64,
        version: SequenceNumber,
    ) -> (
        Arc<PendingWithdraw>,
        oneshot::Receiver<super::super::super::ScheduleResult>,
    ) {
        let (tx, rx) = oneshot::channel();
        let withdraw = TxFundsWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account_id, amount)]),
        };
        (PendingWithdraw::new(version, withdraw, tx), rx)
    }

    #[test]
    fn test_new_account_state() {
        let account_id = make_account_id(1);
        let init_balance = 1000u128;
        let init_version = SequenceNumber::from_u64(5);

        let state = AccountState::new(account_id, init_balance, init_version);

        assert!(state.is_empty());
        assert_eq!(state.last_updated_balance, init_balance);
        assert_eq!(state.last_updated_version, init_version);
    }

    #[test]
    fn test_try_reserve_front_empty_queue() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 1000, SequenceNumber::from_u64(5));

        let result = state.try_reserve_front(SequenceNumber::from_u64(5));
        assert!(!result);
    }

    #[tokio::test]
    async fn test_try_reserve_front_sufficient_funds() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 1000, SequenceNumber::from_u64(5));

        let (pending, rx) = make_pending_withdraw(account_id, 100, SequenceNumber::from_u64(5));
        let result = state.try_reserve_new_withdraw(pending, SequenceNumber::from_u64(5));
        assert!(result);
        assert!(state.pending_reservations.is_empty());
        assert_eq!(
            *state.reserved_funds.reserved_funds.front().unwrap(),
            (SequenceNumber::from_u64(5), 100u128)
        );

        // The sender should have been notified with SufficientFunds
        let schedule_result = rx.await.unwrap();
        assert_eq!(
            schedule_result.status,
            super::super::super::ScheduleStatus::SufficientFunds
        );
    }

    #[test]
    fn test_try_reserve_front_insufficient_funds_not_settled() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 100, SequenceNumber::from_u64(5));

        // Request more than available, but version != last_settled_version
        let (pending, _rx) = make_pending_withdraw(account_id, 200, SequenceNumber::from_u64(6));
        // last_settled_version is 5, pending is at version 6
        let result = state.try_reserve_new_withdraw(pending, SequenceNumber::from_u64(5));
        assert!(!result);
        // The pending withdraw should be pushed back to the front
        assert_eq!(state.pending_reservations.len(), 1);
    }

    #[tokio::test]
    async fn test_try_reserve_front_insufficient_funds_at_settled_version() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 100, SequenceNumber::from_u64(5));

        // Request more than available at settled version
        let (pending, rx) = make_pending_withdraw(account_id, 200, SequenceNumber::from_u64(5));
        // When the pending version equals last_settled_version, insufficient funds triggers notification
        let result = state.try_reserve_new_withdraw(pending, SequenceNumber::from_u64(5));
        assert!(result);
        assert!(state.pending_reservations.is_empty());

        let schedule_result = rx.await.unwrap();
        assert_eq!(
            schedule_result.status,
            super::super::super::ScheduleStatus::InsufficientFunds
        );
    }

    #[tokio::test]
    async fn test_try_reserve_front_multiple_reserves_exact_balance() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 300, SequenceNumber::from_u64(5));

        let (p1, rx1) = make_pending_withdraw(account_id, 100, SequenceNumber::from_u64(5));
        let (p2, rx2) = make_pending_withdraw(account_id, 100, SequenceNumber::from_u64(5));
        let (p3, rx3) = make_pending_withdraw(account_id, 100, SequenceNumber::from_u64(5));

        // Reserve all three
        assert!(state.try_reserve_new_withdraw(p1, SequenceNumber::from_u64(5)));
        assert!(state.try_reserve_new_withdraw(p2, SequenceNumber::from_u64(5)));
        assert!(state.try_reserve_new_withdraw(p3, SequenceNumber::from_u64(5)));

        // All reserved exactly 300
        assert_eq!(
            *state.reserved_funds.reserved_funds.front().unwrap(),
            (SequenceNumber::from_u64(5), 300u128)
        );
        assert!(state.pending_reservations.is_empty());

        // All should be SufficientFunds
        assert_eq!(
            rx1.await.unwrap().status,
            super::super::super::ScheduleStatus::SufficientFunds
        );
        assert_eq!(
            rx2.await.unwrap().status,
            super::super::super::ScheduleStatus::SufficientFunds
        );
        assert_eq!(
            rx3.await.unwrap().status,
            super::super::super::ScheduleStatus::SufficientFunds
        );
    }

    #[tokio::test]
    async fn test_try_reserve_front_exceeds_balance_after_reservations() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 150, SequenceNumber::from_u64(5));

        let (p1, rx1) = make_pending_withdraw(account_id, 100, SequenceNumber::from_u64(5));
        let (p2, rx2) = make_pending_withdraw(account_id, 100, SequenceNumber::from_u64(5));

        // First one succeeds
        assert!(state.try_reserve_new_withdraw(p1, SequenceNumber::from_u64(5)));
        assert_eq!(
            rx1.await.unwrap().status,
            super::super::super::ScheduleStatus::SufficientFunds
        );

        // Second one fails (100 reserved + 100 needed = 200 > 150)
        assert!(state.try_reserve_new_withdraw(p2, SequenceNumber::from_u64(5)));
        assert_eq!(
            rx2.await.unwrap().status,
            super::super::super::ScheduleStatus::InsufficientFunds
        );
    }

    #[test]
    fn test_settle_funds_increases_balance() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 1000, SequenceNumber::from_u64(5));

        state.settle_funds(500, SequenceNumber::from_u64(6));

        assert_eq!(state.last_updated_balance, 1500);
        assert_eq!(state.last_updated_version, SequenceNumber::from_u64(6));
    }

    #[test]
    fn test_settle_funds_decreases_balance() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 1000, SequenceNumber::from_u64(5));

        state.settle_funds(-300, SequenceNumber::from_u64(6));

        assert_eq!(state.last_updated_balance, 700);
        assert_eq!(state.last_updated_version, SequenceNumber::from_u64(6));
    }

    #[test]
    #[should_panic]
    fn test_settle_funds_negative_balance_panics() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 100, SequenceNumber::from_u64(5));

        // This should panic because 100 - 200 = -100 < 0
        state.settle_funds(-200, SequenceNumber::from_u64(6));
    }

    #[test]
    fn test_settle_funds_same_version_no_update() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 1000, SequenceNumber::from_u64(5));

        // Same version should not update balance
        state.settle_funds(500, SequenceNumber::from_u64(5));

        assert_eq!(state.last_updated_balance, 1000);
        assert_eq!(state.last_updated_version, SequenceNumber::from_u64(5));
    }

    #[test]
    fn test_settle_funds_older_version_no_update() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 1000, SequenceNumber::from_u64(5));

        // Older version should not update balance
        state.settle_funds(500, SequenceNumber::from_u64(3));

        assert_eq!(state.last_updated_balance, 1000);
        assert_eq!(state.last_updated_version, SequenceNumber::from_u64(5));
    }

    #[test]
    fn test_settle_funds_clears_old_reserved_funds() {
        let mut reserved_funds = ReservedFunds::default();
        // Manually add some reserved funds at different versions
        reserved_funds.add_reserved_fund(SequenceNumber::from_u64(5), 100);
        reserved_funds.add_reserved_fund(SequenceNumber::from_u64(6), 200);
        reserved_funds.add_reserved_fund(SequenceNumber::from_u64(7), 300);

        // Settle at version 6 should remove version 5 reserved funds
        reserved_funds.settle_fund(SequenceNumber::from_u64(6));

        assert_eq!(reserved_funds.reserved_funds.len(), 2);
        assert_eq!(
            reserved_funds.reserved_funds.front(),
            Some(&(SequenceNumber::from_u64(6), 200))
        );
        assert_eq!(
            reserved_funds.reserved_funds.back(),
            Some(&(SequenceNumber::from_u64(7), 300))
        );
    }

    #[tokio::test]
    async fn test_settle_funds_processes_pending_reservations() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 100, SequenceNumber::from_u64(5));

        // Add a withdraw that needs 200 (more than current balance)
        let (pending, rx) = make_pending_withdraw(account_id, 200, SequenceNumber::from_u64(6));
        // Try to reserve - should fail because insufficient funds
        assert!(!state.try_reserve_new_withdraw(pending, SequenceNumber::from_u64(5)));
        assert_eq!(state.pending_reservations.len(), 1);

        // Settle with +150, bringing balance to 250 at version 6
        state.settle_funds(150, SequenceNumber::from_u64(6));

        // The pending reservation should have been automatically processed
        assert!(state.pending_reservations.is_empty());
        assert_eq!(
            *state.reserved_funds.reserved_funds.front().unwrap(),
            (SequenceNumber::from_u64(6), 200)
        );

        let result = rx.await.unwrap();
        assert_eq!(
            result.status,
            super::super::super::ScheduleStatus::SufficientFunds
        );
    }

    #[test]
    fn test_is_empty_with_reserved_funds() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 1000, SequenceNumber::from_u64(5));

        state
            .reserved_funds
            .add_reserved_fund(SequenceNumber::from_u64(5), 100);

        assert!(!state.is_empty());
    }

    #[tokio::test]
    async fn test_reserved_funds_accumulated_across_versions() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 500, SequenceNumber::from_u64(5));

        let (p1, _rx1) = make_pending_withdraw(account_id, 200, SequenceNumber::from_u64(5));
        let (p2, _rx2) = make_pending_withdraw(account_id, 150, SequenceNumber::from_u64(6));

        // Reserve first at version 5
        assert!(state.try_reserve_new_withdraw(p1, SequenceNumber::from_u64(5)));
        assert_eq!(
            *state.reserved_funds.reserved_funds.front().unwrap(),
            (SequenceNumber::from_u64(5), 200)
        );

        // Reserve second at version 6
        assert!(state.try_reserve_new_withdraw(p2, SequenceNumber::from_u64(5)));
        assert_eq!(
            *state.reserved_funds.reserved_funds.back().unwrap(),
            (SequenceNumber::from_u64(6), 150)
        );

        // Total reserved is 350, which is within the 500 balance
    }

    #[tokio::test]
    async fn test_reserved_funds_across_versions_insufficient() {
        let account_id = make_account_id(1);
        let mut state = AccountState::new(account_id, 300, SequenceNumber::from_u64(5));

        let (p1, rx1) = make_pending_withdraw(account_id, 200, SequenceNumber::from_u64(5));
        let (p2, _rx2) = make_pending_withdraw(account_id, 150, SequenceNumber::from_u64(6));

        // Reserve first (200) - should succeed
        assert!(state.try_reserve_new_withdraw(p1, SequenceNumber::from_u64(5)));
        assert_eq!(
            rx1.await.unwrap().status,
            super::super::super::ScheduleStatus::SufficientFunds
        );

        // Try to reserve second (150) - but 200 + 150 = 350 > 300
        // Version 6 != last_settled_version (5), so it should return false
        assert!(!state.try_reserve_new_withdraw(p2, SequenceNumber::from_u64(5)));
        assert_eq!(state.pending_reservations.len(), 1);
    }
}
