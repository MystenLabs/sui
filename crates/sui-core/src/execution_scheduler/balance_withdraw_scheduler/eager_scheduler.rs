// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use parking_lot::RwLock;
use prometheus::{
    register_int_counter_vec_with_registry, register_int_gauge_vec_with_registry, IntCounterVec,
    IntGaugeVec, Registry,
};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    transaction::Reservation,
};
use tokio::sync::oneshot;
use tracing::{debug, trace};

use crate::execution_scheduler::balance_withdraw_scheduler::{
    balance_read::AccountBalanceRead,
    scheduler::{BalanceWithdrawSchedulerTrait, WithdrawReservations},
    BalanceSettlement, ScheduleResult, ScheduleStatus, TxBalanceWithdraw,
};

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub(super) struct AccountState {
    /// The last known settled balance from the accumulator
    settled_balance: u64,
    /// Cumulative reservations since the last known settlement
    /// This tracks the sum of all reservations scheduled since the last settlement
    cumulative_reservations: u64,
    /// The accumulator version at which this balance was last settled
    last_settled_version: SequenceNumber,
    /// Whether an EntireBalance reservation has been made for this account
    /// If true, no further reservations can be scheduled until settlement
    entire_balance_reserved: bool,
}

impl AccountState {
    pub(super) fn new(balance: u64, version: SequenceNumber) -> Self {
        Self {
            settled_balance: balance,
            cumulative_reservations: 0,
            last_settled_version: version,
            entire_balance_reserved: false,
        }
    }

    /// Calculate the minimum guaranteed balance available for new reservations
    pub(super) fn minimum_guaranteed_balance(&self) -> u64 {
        if self.entire_balance_reserved {
            0
        } else {
            self.settled_balance
                .saturating_sub(self.cumulative_reservations)
        }
    }

    /// Try to reserve an amount from this account
    /// Returns true if the reservation was successful
    pub(super) fn try_reserve(&mut self, reservation: &Reservation) -> bool {
        match reservation {
            Reservation::MaxAmountU64(amount) => {
                if self.entire_balance_reserved {
                    return false;
                }
                let available = self.minimum_guaranteed_balance();
                if available >= *amount {
                    self.cumulative_reservations =
                        self.cumulative_reservations.saturating_add(*amount);
                    true
                } else {
                    false
                }
            }
            Reservation::EntireBalance => {
                if self.entire_balance_reserved || self.cumulative_reservations > 0 {
                    false
                } else if self.settled_balance > 0 {
                    self.entire_balance_reserved = true;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Apply a settlement to this account
    pub(super) fn apply_settlement(&mut self, new_balance: u64, version: SequenceNumber) {
        self.settled_balance = new_balance;
        self.cumulative_reservations = 0;
        self.entire_balance_reserved = false;
        self.last_settled_version = version;
    }
}

/// Tracks which consensus commit batches have been scheduled to prevent double scheduling
#[derive(Debug)]
struct ScheduledBatches {
    /// Maps accumulator version to whether that batch has been scheduled
    scheduled_versions: BTreeMap<SequenceNumber, bool>,
}

impl ScheduledBatches {
    fn new() -> Self {
        Self {
            scheduled_versions: BTreeMap::new(),
        }
    }

    /// Check if a batch has already been scheduled
    fn is_already_scheduled(&self, version: SequenceNumber) -> bool {
        self.scheduled_versions.contains_key(&version)
    }

    /// Mark a batch as scheduled
    fn mark_scheduled(&mut self, version: SequenceNumber) {
        self.scheduled_versions.insert(version, true);
    }

    /// Clean up old entries that are before the given version
    fn cleanup_before(&mut self, version: SequenceNumber) {
        self.scheduled_versions = self.scheduled_versions.split_off(&version);
    }
}

/// Metrics for tracking scheduler performance and behavior
pub struct EagerSchedulerMetrics {
    /// Count of scheduling outcomes by status
    pub schedule_outcome_counter: IntCounterVec,
    /// Number of accounts currently being tracked
    pub tracked_accounts_gauge: IntGaugeVec,
    /// Number of active reservations by type
    pub active_reservations_gauge: IntGaugeVec,
    /// Count of settlements processed
    pub settlements_processed_counter: IntCounterVec,
}

impl EagerSchedulerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            schedule_outcome_counter: register_int_counter_vec_with_registry!(
                "eager_scheduler_schedule_outcome",
                "Count of scheduling outcomes by status",
                &["status"],
                registry,
            )
            .unwrap(),
            tracked_accounts_gauge: register_int_gauge_vec_with_registry!(
                "eager_scheduler_tracked_accounts",
                "Number of accounts currently being tracked",
                &["type"],
                registry,
            )
            .unwrap(),
            active_reservations_gauge: register_int_gauge_vec_with_registry!(
                "eager_scheduler_active_reservations",
                "Number of active reservations by type",
                &["type"],
                registry,
            )
            .unwrap(),
            settlements_processed_counter: register_int_counter_vec_with_registry!(
                "eager_scheduler_settlements_processed",
                "Count of settlements processed",
                &["type"],
                registry,
            )
            .unwrap(),
        }
    }
}

/// The eager balance withdrawal scheduler that optimistically schedules withdrawals
/// without waiting for settlements when sufficient balance can be guaranteed
pub(crate) struct EagerBalanceWithdrawScheduler {
    balance_read: Arc<dyn AccountBalanceRead>,
    /// Protected state that tracks account balances and reservations
    state: Arc<RwLock<EagerSchedulerState>>,
    /// Metrics for monitoring
    metrics: Option<EagerSchedulerMetrics>,
}

struct EagerSchedulerState {
    /// Track account states only for accounts with pending withdrawals
    account_states: BTreeMap<ObjectID, AccountState>,
    /// Track which consensus batches have been scheduled
    scheduled_batches: ScheduledBatches,
    /// The highest accumulator version we've processed
    highest_processed_version: SequenceNumber,
    /// The last settled accumulator version
    last_settled_version: SequenceNumber,
    /// Pending transactions that couldn't be scheduled due to insufficient balance
    /// Maps accumulator version -> list of (transaction, sender) pairs waiting for settlement
    pending_insufficient_balance: BTreeMap<SequenceNumber, Vec<(TxBalanceWithdraw, oneshot::Sender<ScheduleResult>)>>,
}

impl EagerBalanceWithdrawScheduler {
    pub fn new(
        balance_read: Arc<dyn AccountBalanceRead>,
        starting_accumulator_version: SequenceNumber,
    ) -> Arc<Self> {
        Arc::new(Self {
            balance_read,
            state: Arc::new(RwLock::new(EagerSchedulerState {
                account_states: BTreeMap::new(),
                scheduled_batches: ScheduledBatches::new(),
                highest_processed_version: starting_accumulator_version,
                last_settled_version: starting_accumulator_version,
                pending_insufficient_balance: BTreeMap::new(),
            })),
            metrics: None,
        })
    }

    pub fn new_with_metrics(
        balance_read: Arc<dyn AccountBalanceRead>,
        starting_accumulator_version: SequenceNumber,
        registry: &Registry,
    ) -> Arc<Self> {
        Arc::new(Self {
            balance_read,
            state: Arc::new(RwLock::new(EagerSchedulerState {
                account_states: BTreeMap::new(),
                scheduled_batches: ScheduledBatches::new(),
                highest_processed_version: starting_accumulator_version,
                last_settled_version: starting_accumulator_version,
                pending_insufficient_balance: BTreeMap::new(),
            })),
            metrics: Some(EagerSchedulerMetrics::new(registry)),
        })
    }

    /// Load balance for an account if not already tracked
    fn ensure_account_loaded(
        &self,
        state: &mut EagerSchedulerState,
        account_id: &ObjectID,
        accumulator_version: SequenceNumber,
    ) {
        if !state.account_states.contains_key(account_id) {
            let balance = self
                .balance_read
                .get_account_balance(account_id, accumulator_version);
            state
                .account_states
                .insert(*account_id, AccountState::new(balance, accumulator_version));
            trace!(
                "Loaded account {:?} with balance {} at version {:?}",
                account_id,
                balance,
                accumulator_version
            );
        }
    }

    /// Clean up accounts that no longer need tracking
    fn cleanup_accounts(&self, state: &mut EagerSchedulerState) {
        let _before_count = state.account_states.len();
        state.account_states.retain(|account_id, account_state| {
            let should_retain =
                account_state.cumulative_reservations > 0 || account_state.entire_balance_reserved;
            if !should_retain {
                trace!("Removing account {:?} from tracking", account_id);
            }
            should_retain
        });

        if let Some(metrics) = &self.metrics {
            metrics
                .tracked_accounts_gauge
                .with_label_values(&["total"])
                .set(state.account_states.len() as i64);

            let with_reservations = state
                .account_states
                .values()
                .filter(|s| s.cumulative_reservations > 0)
                .count();
            metrics
                .tracked_accounts_gauge
                .with_label_values(&["with_reservations"])
                .set(with_reservations as i64);

            let entire_balance_reserved = state
                .account_states
                .values()
                .filter(|s| s.entire_balance_reserved)
                .count();
            metrics
                .tracked_accounts_gauge
                .with_label_values(&["entire_balance_reserved"])
                .set(entire_balance_reserved as i64);
        }
    }
}

#[async_trait::async_trait]
impl BalanceWithdrawSchedulerTrait for EagerBalanceWithdrawScheduler {
    async fn schedule_withdraws(&self, withdraws: WithdrawReservations) {
        let mut state = self.state.write();

        // Check if this version has already been settled
        if withdraws.accumulator_version <= state.last_settled_version {
            debug!(
                "Batch at version {:?} already settled (last settled: {:?})",
                withdraws.accumulator_version, state.last_settled_version
            );
            for (withdraw, sender) in withdraws.withdraws.into_iter().zip(withdraws.senders) {
                let _ = sender.send(ScheduleResult {
                    tx_digest: withdraw.tx_digest,
                    status: ScheduleStatus::AlreadyExecuted,
                });
                if let Some(metrics) = &self.metrics {
                    metrics
                        .schedule_outcome_counter
                        .with_label_values(&["already_executed"])
                        .inc();
                }
            }
            return;
        }

        // Check if this batch has already been scheduled
        if state
            .scheduled_batches
            .is_already_scheduled(withdraws.accumulator_version)
        {
            debug!(
                "Batch at version {:?} already scheduled",
                withdraws.accumulator_version
            );
            for (withdraw, sender) in withdraws.withdraws.into_iter().zip(withdraws.senders) {
                let _ = sender.send(ScheduleResult {
                    tx_digest: withdraw.tx_digest,
                    status: ScheduleStatus::AlreadyExecuted,
                });
                if let Some(metrics) = &self.metrics {
                    metrics
                        .schedule_outcome_counter
                        .with_label_values(&["already_executed"])
                        .inc();
                }
            }
            return;
        }

        // Mark this batch as scheduled
        state
            .scheduled_batches
            .mark_scheduled(withdraws.accumulator_version);
        state.highest_processed_version = state
            .highest_processed_version
            .max(withdraws.accumulator_version);

        // Process each transaction's withdrawals sequentially
        for (withdraw, sender) in withdraws.withdraws.into_iter().zip(withdraws.senders) {
            // First ensure all accounts in this transaction are loaded
            for account_id in withdraw.reservations.keys() {
                self.ensure_account_loaded(&mut state, account_id, withdraws.accumulator_version);
            }

            // Try to reserve all amounts atomically for this transaction
            let mut temp_states = Vec::new();
            let mut all_success = true;

            for (account_id, reservation) in &withdraw.reservations {
                let account_state = state.account_states.get_mut(account_id).unwrap();
                let original_state = account_state.clone();

                if account_state.try_reserve(reservation) {
                    temp_states.push((*account_id, original_state));
                } else {
                    all_success = false;
                    // Rollback any partial reservations
                    for (rollback_id, original) in temp_states {
                        *state.account_states.get_mut(&rollback_id).unwrap() = original;
                    }
                    break;
                }
            }

            if all_success {
                debug!(
                    "Successfully scheduled withdraw {:?} with reservations {:?}",
                    withdraw.tx_digest, withdraw.reservations
                );
                let _ = sender.send(ScheduleResult {
                    tx_digest: withdraw.tx_digest,
                    status: ScheduleStatus::SufficientBalance,
                });
                if let Some(metrics) = &self.metrics {
                    metrics
                        .schedule_outcome_counter
                        .with_label_values(&["sufficient_balance"])
                        .inc();
                }
            } else {
                // For eager scheduling, we don't know yet if there might be more deposits
                // coming in later versions, so we need to hold the InsufficientBalance
                // decision until settlement
                debug!(
                    "Pending insufficient balance decision for withdraw {:?} with reservations {:?}",
                    withdraw.tx_digest, withdraw.reservations
                );
                state
                    .pending_insufficient_balance
                    .entry(withdraws.accumulator_version)
                    .or_default()
                    .push((withdraw, sender));
                if let Some(metrics) = &self.metrics {
                    metrics
                        .schedule_outcome_counter
                        .with_label_values(&["pending_insufficient"])
                        .inc();
                }
            }
        }

        // Clean up accounts that no longer need tracking
        self.cleanup_accounts(&mut state);
    }

    async fn settle_balances(&self, settlement: BalanceSettlement) {
        let mut state = self.state.write();

        debug!(
            "Settling balances at version {:?} with {} changes",
            settlement.accumulator_version,
            settlement.balance_changes.len()
        );

        if let Some(metrics) = &self.metrics {
            metrics
                .settlements_processed_counter
                .with_label_values(&["total"])
                .inc();

            if !settlement.balance_changes.is_empty() {
                metrics
                    .settlements_processed_counter
                    .with_label_values(&["with_changes"])
                    .inc();
            }
        }

        // Update the last settled version
        state.last_settled_version = settlement.accumulator_version;

        // Apply balance changes to tracked accounts
        for (account_id, balance_change) in &settlement.balance_changes {
            if let Some(account_state) = state.account_states.get_mut(account_id) {
                // Calculate new balance from the change
                let new_balance = if *balance_change >= 0 {
                    account_state
                        .settled_balance
                        .saturating_add(*balance_change as u64)
                } else {
                    account_state
                        .settled_balance
                        .saturating_sub(balance_change.unsigned_abs() as u64)
                };

                account_state.apply_settlement(new_balance, settlement.accumulator_version);
                trace!(
                    "Applied settlement to account {:?}: change={}, new_balance={}",
                    account_id,
                    balance_change,
                    new_balance
                );
            }
        }

        // For any tracked accounts not in the settlement, we need to update their version
        // and fetch the latest balance
        let accounts_to_update: Vec<ObjectID> = state
            .account_states
            .iter()
            .filter(|(id, _)| !settlement.balance_changes.contains_key(id))
            .map(|(id, _)| *id)
            .collect();

        for account_id in accounts_to_update {
            let new_balance = self
                .balance_read
                .get_account_balance(&account_id, settlement.accumulator_version);
            if let Some(account_state) = state.account_states.get_mut(&account_id) {
                account_state.apply_settlement(new_balance, settlement.accumulator_version);
                trace!(
                    "Refreshed balance for account {:?}: new_balance={}",
                    account_id,
                    new_balance
                );
            }
        }

        // Process any pending insufficient balance transactions for this version
        if let Some(pending_txs) = state.pending_insufficient_balance.remove(&settlement.accumulator_version) {
            debug!(
                "Processing {} pending insufficient balance transactions for version {:?}",
                pending_txs.len(),
                settlement.accumulator_version
            );
            
            for (withdraw, sender) in pending_txs {
                // Re-check if we can now schedule the transaction with the settled balances
                let mut all_success = true;
                let mut temp_states = Vec::new();
                
                for (account_id, reservation) in &withdraw.reservations {
                    if let Some(account_state) = state.account_states.get_mut(account_id) {
                        let original_state = account_state.clone();
                        
                        if account_state.try_reserve(reservation) {
                            temp_states.push((*account_id, original_state));
                        } else {
                            all_success = false;
                            // Rollback any partial reservations
                            for (rollback_id, original) in temp_states {
                                *state.account_states.get_mut(&rollback_id).unwrap() = original;
                            }
                            break;
                        }
                    } else {
                        // Account not tracked - load it with current settlement balance
                        let balance = self
                            .balance_read
                            .get_account_balance(account_id, settlement.accumulator_version);
                        state
                            .account_states
                            .insert(*account_id, AccountState::new(balance, settlement.accumulator_version));
                        
                        let account_state = state.account_states.get_mut(account_id).unwrap();
                        let original_state = account_state.clone();
                        
                        if account_state.try_reserve(reservation) {
                            temp_states.push((*account_id, original_state));
                        } else {
                            all_success = false;
                            // Rollback any partial reservations
                            for (rollback_id, original) in temp_states {
                                *state.account_states.get_mut(&rollback_id).unwrap() = original;
                            }
                            break;
                        }
                    }
                }
                
                let status = if all_success {
                    debug!(
                        "Pending transaction {:?} now has sufficient balance after settlement",
                        withdraw.tx_digest
                    );
                    ScheduleStatus::SufficientBalance
                } else {
                    debug!(
                        "Pending transaction {:?} confirmed insufficient balance after settlement",
                        withdraw.tx_digest
                    );
                    ScheduleStatus::InsufficientBalance
                };
                
                let _ = sender.send(ScheduleResult {
                    tx_digest: withdraw.tx_digest,
                    status,
                });
                
                if let Some(metrics) = &self.metrics {
                    let label = match status {
                        ScheduleStatus::SufficientBalance => "settled_sufficient",
                        ScheduleStatus::InsufficientBalance => "settled_insufficient",
                        _ => unreachable!(),
                    };
                    metrics
                        .schedule_outcome_counter
                        .with_label_values(&[label])
                        .inc();
                }
            }
        }

        // Clean up old scheduled batch entries
        state
            .scheduled_batches
            .cleanup_before(settlement.accumulator_version);

        // Clean up accounts that no longer need tracking
        self.cleanup_accounts(&mut state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_scheduler::balance_withdraw_scheduler::balance_read::AccountBalanceRead;
    use std::sync::Mutex;
    use sui_types::digests::TransactionDigest;
    use tokio::sync::oneshot;

    // Simple mock for testing
    struct SimpleMockBalanceRead {
        balances: Arc<Mutex<BTreeMap<ObjectID, u64>>>,
    }

    impl SimpleMockBalanceRead {
        fn new(initial_balances: BTreeMap<ObjectID, u64>) -> Self {
            Self {
                balances: Arc::new(Mutex::new(initial_balances)),
            }
        }
    }

    impl AccountBalanceRead for SimpleMockBalanceRead {
        fn get_account_balance(
            &self,
            account_id: &ObjectID,
            _accumulator_version: SequenceNumber,
        ) -> u64 {
            self.balances
                .lock()
                .unwrap()
                .get(account_id)
                .copied()
                .unwrap_or(0)
        }
    }

    #[test]
    fn test_account_state_reservations() {
        let mut state = AccountState::new(100, SequenceNumber::from_u64(0));

        // Test MaxAmountU64 reservation
        assert!(state.try_reserve(&Reservation::MaxAmountU64(50)));
        assert_eq!(state.minimum_guaranteed_balance(), 50);
        assert!(state.try_reserve(&Reservation::MaxAmountU64(30)));
        assert_eq!(state.minimum_guaranteed_balance(), 20);
        assert!(!state.try_reserve(&Reservation::MaxAmountU64(30)));

        // Test EntireBalance reservation
        let mut state2 = AccountState::new(100, SequenceNumber::from_u64(0));
        assert!(state2.try_reserve(&Reservation::EntireBalance));
        assert_eq!(state2.minimum_guaranteed_balance(), 0);
        assert!(!state2.try_reserve(&Reservation::MaxAmountU64(1)));

        // Test EntireBalance after partial reservation
        let mut state3 = AccountState::new(100, SequenceNumber::from_u64(0));
        assert!(state3.try_reserve(&Reservation::MaxAmountU64(50)));
        assert!(!state3.try_reserve(&Reservation::EntireBalance));
    }

    #[test]
    fn test_settlement_resets_reservations() {
        let mut state = AccountState::new(100, SequenceNumber::from_u64(0));
        assert!(state.try_reserve(&Reservation::MaxAmountU64(80)));
        assert_eq!(state.minimum_guaranteed_balance(), 20);

        state.apply_settlement(150, SequenceNumber::from_u64(1));
        assert_eq!(state.minimum_guaranteed_balance(), 150);
        assert_eq!(state.cumulative_reservations, 0);
        assert!(!state.entire_balance_reserved);
    }

    #[tokio::test]
    async fn test_already_settled_version() {
        let balance_read = Arc::new(SimpleMockBalanceRead::new(BTreeMap::new()));
        let scheduler = EagerBalanceWithdrawScheduler::new(
            balance_read,
            SequenceNumber::from_u64(10),
        );

        // Settle a version
        let settlement = BalanceSettlement {
            accumulator_version: SequenceNumber::from_u64(15),
            balance_changes: BTreeMap::new(),
        };
        scheduler.settle_balances(settlement).await;

        // Try to schedule a transaction for an already settled version
        let account = ObjectID::random();
        let tx_digest = TransactionDigest::random();
        let withdraw = TxBalanceWithdraw {
            tx_digest,
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(50))]),
        };
        let (sender, receiver) = oneshot::channel();
        let withdraws = WithdrawReservations {
            accumulator_version: SequenceNumber::from_u64(12), // Less than settled version
            withdraws: vec![withdraw],
            senders: vec![sender],
        };

        scheduler.schedule_withdraws(withdraws).await;

        let result = receiver.await.unwrap();
        assert_eq!(result.status, ScheduleStatus::AlreadyExecuted);
    }

    #[tokio::test]
    async fn test_pending_insufficient_balance_becomes_sufficient() {
        let balance_read = Arc::new(SimpleMockBalanceRead::new(BTreeMap::from([
            (ObjectID::from_hex_literal("0x1").unwrap(), 50),
        ])));
        let scheduler = EagerBalanceWithdrawScheduler::new(
            balance_read.clone(),
            SequenceNumber::from_u64(0),
        );

        let account = ObjectID::from_hex_literal("0x1").unwrap();
        let tx_digest = TransactionDigest::random();
        
        // Try to withdraw more than available balance
        let withdraw = TxBalanceWithdraw {
            tx_digest,
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(100))]),
        };
        let (sender, mut receiver) = oneshot::channel();
        let withdraws = WithdrawReservations {
            accumulator_version: SequenceNumber::from_u64(1),
            withdraws: vec![withdraw.clone()],
            senders: vec![sender],
        };

        scheduler.schedule_withdraws(withdraws).await;

        // Should not receive result yet (pending settlement)
        assert!(receiver.try_recv().is_err());

        // Settle with updated balance
        balance_read.balances.lock().unwrap().insert(account, 150);
        let settlement = BalanceSettlement {
            accumulator_version: SequenceNumber::from_u64(1),
            balance_changes: BTreeMap::from([(account, 100)]), // Deposit of 100
        };
        scheduler.settle_balances(settlement).await;

        // Now we should receive SufficientBalance
        let result = receiver.await.unwrap();
        assert_eq!(result.status, ScheduleStatus::SufficientBalance);
    }

    #[tokio::test]
    async fn test_confirmed_insufficient_balance_after_settlement() {
        let balance_read = Arc::new(SimpleMockBalanceRead::new(BTreeMap::from([
            (ObjectID::from_hex_literal("0x1").unwrap(), 50),
        ])));
        let scheduler = EagerBalanceWithdrawScheduler::new(
            balance_read,
            SequenceNumber::from_u64(0),
        );

        let account = ObjectID::from_hex_literal("0x1").unwrap();
        let tx_digest = TransactionDigest::random();
        
        // Try to withdraw more than available balance
        let withdraw = TxBalanceWithdraw {
            tx_digest,
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(100))]),
        };
        let (sender, mut receiver) = oneshot::channel();
        let withdraws = WithdrawReservations {
            accumulator_version: SequenceNumber::from_u64(1),
            withdraws: vec![withdraw],
            senders: vec![sender],
        };

        scheduler.schedule_withdraws(withdraws).await;

        // Should not receive result yet (pending settlement)
        assert!(receiver.try_recv().is_err());

        // Settle without balance change - confirms insufficient balance
        let settlement = BalanceSettlement {
            accumulator_version: SequenceNumber::from_u64(1),
            balance_changes: BTreeMap::new(),
        };
        scheduler.settle_balances(settlement).await;

        // Now we should receive InsufficientBalance
        let result = receiver.await.unwrap();
        assert_eq!(result.status, ScheduleStatus::InsufficientBalance);
    }

    #[tokio::test]
    async fn test_multiple_pending_transactions_mixed_outcomes() {
        // Test multiple transactions with different outcomes after settlement
        let initial_balances = BTreeMap::from([
            (ObjectID::from_hex_literal("0x1").unwrap(), 100),
            (ObjectID::from_hex_literal("0x2").unwrap(), 50),
            (ObjectID::from_hex_literal("0x3").unwrap(), 200),
        ]);
        let balance_read = Arc::new(SimpleMockBalanceRead::new(initial_balances.clone()));
        let scheduler = EagerBalanceWithdrawScheduler::new(
            balance_read.clone(),
            SequenceNumber::from_u64(0),
        );

        let account1 = ObjectID::from_hex_literal("0x1").unwrap();
        let account2 = ObjectID::from_hex_literal("0x2").unwrap();
        let account3 = ObjectID::from_hex_literal("0x3").unwrap();

        // Transaction 1: Will succeed after settlement (needs 150, has 100, will get 100 more)
        let tx1 = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account1, Reservation::MaxAmountU64(150))]),
        };
        let (sender1, mut receiver1) = oneshot::channel();

        // Transaction 2: Will fail after settlement (needs 100, has 50, will lose 10)
        let tx2 = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account2, Reservation::MaxAmountU64(100))]),
        };
        let (sender2, mut receiver2) = oneshot::channel();

        // Transaction 3: Already has sufficient balance
        let tx3 = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account3, Reservation::MaxAmountU64(150))]),
        };
        let (sender3, receiver3) = oneshot::channel();

        let withdraws = WithdrawReservations {
            accumulator_version: SequenceNumber::from_u64(1),
            withdraws: vec![tx1, tx2, tx3],
            senders: vec![sender1, sender2, sender3],
        };

        scheduler.schedule_withdraws(withdraws).await;

        // Transaction 3 should succeed immediately
        let result3 = receiver3.await.unwrap();
        assert_eq!(result3.status, ScheduleStatus::SufficientBalance);

        // Transactions 1 and 2 should be pending
        assert!(receiver1.try_recv().is_err());
        assert!(receiver2.try_recv().is_err());

        // Update balances and settle
        {
            let mut balances = balance_read.balances.lock().unwrap();
            balances.insert(account1, 200); // +100
            balances.insert(account2, 40);  // -10
        }

        let settlement = BalanceSettlement {
            accumulator_version: SequenceNumber::from_u64(1),
            balance_changes: BTreeMap::from([
                (account1, 100),  // Deposit
                (account2, -10),  // Withdrawal
            ]),
        };
        scheduler.settle_balances(settlement).await;

        // Transaction 1 should now succeed
        let result1 = receiver1.await.unwrap();
        assert_eq!(result1.status, ScheduleStatus::SufficientBalance);

        // Transaction 2 should fail
        let result2 = receiver2.await.unwrap();
        assert_eq!(result2.status, ScheduleStatus::InsufficientBalance);
    }

    #[tokio::test]
    async fn test_pending_entire_balance_reservation() {
        // Test EntireBalance reservation that becomes possible after settlement
        let balance_read = Arc::new(SimpleMockBalanceRead::new(BTreeMap::from([
            (ObjectID::from_hex_literal("0x1").unwrap(), 0), // Empty account
        ])));
        let scheduler = EagerBalanceWithdrawScheduler::new(
            balance_read.clone(),
            SequenceNumber::from_u64(0),
        );

        let account = ObjectID::from_hex_literal("0x1").unwrap();
        
        // Try to reserve entire balance on empty account
        let withdraw = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::EntireBalance)]),
        };
        let (sender, mut receiver) = oneshot::channel();
        let withdraws = WithdrawReservations {
            accumulator_version: SequenceNumber::from_u64(1),
            withdraws: vec![withdraw],
            senders: vec![sender],
        };

        scheduler.schedule_withdraws(withdraws).await;

        // Should be pending because balance is 0
        assert!(receiver.try_recv().is_err());

        // Deposit some funds
        balance_read.balances.lock().unwrap().insert(account, 100);
        let settlement = BalanceSettlement {
            accumulator_version: SequenceNumber::from_u64(1),
            balance_changes: BTreeMap::from([(account, 100)]),
        };
        scheduler.settle_balances(settlement).await;

        // Should now succeed
        let result = receiver.await.unwrap();
        assert_eq!(result.status, ScheduleStatus::SufficientBalance);
    }

    #[tokio::test]
    async fn test_multiple_versions_with_pending_decisions() {
        // Test transactions across multiple versions with cascading effects
        let balance_read = Arc::new(SimpleMockBalanceRead::new(BTreeMap::from([
            (ObjectID::from_hex_literal("0x1").unwrap(), 100),
        ])));
        let scheduler = EagerBalanceWithdrawScheduler::new(
            balance_read.clone(),
            SequenceNumber::from_u64(0),
        );

        let account = ObjectID::from_hex_literal("0x1").unwrap();

        // Version 1: Successful withdrawal of 30
        let tx1 = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(30))]),
        };
        let (sender1, receiver1) = oneshot::channel();
        scheduler.schedule_withdraws(WithdrawReservations {
            accumulator_version: SequenceNumber::from_u64(1),
            withdraws: vec![tx1],
            senders: vec![sender1],
        }).await;
        assert_eq!(receiver1.await.unwrap().status, ScheduleStatus::SufficientBalance);

        // Version 2: Try to withdraw 80 (would need balance > 70, but only has 70 left)
        let tx2 = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(80))]),
        };
        let (sender2, mut receiver2) = oneshot::channel();
        scheduler.schedule_withdraws(WithdrawReservations {
            accumulator_version: SequenceNumber::from_u64(2),
            withdraws: vec![tx2],
            senders: vec![sender2],
        }).await;
        
        // Should be pending
        assert!(receiver2.try_recv().is_err());

        // Settle version 1 (balance becomes 70 after withdrawal)
        balance_read.balances.lock().unwrap().insert(account, 70);
        scheduler.settle_balances(BalanceSettlement {
            accumulator_version: SequenceNumber::from_u64(1),
            balance_changes: BTreeMap::from([(account, -30)]),
        }).await;

        // Version 2 transaction should still be pending
        assert!(receiver2.try_recv().is_err());

        // Settle version 2 with a deposit
        balance_read.balances.lock().unwrap().insert(account, 120); // +50 deposit
        scheduler.settle_balances(BalanceSettlement {
            accumulator_version: SequenceNumber::from_u64(2),
            balance_changes: BTreeMap::from([(account, 50)]),
        }).await;

        // Now it should succeed
        assert_eq!(receiver2.await.unwrap().status, ScheduleStatus::SufficientBalance);
    }

    #[tokio::test]
    async fn test_transaction_with_multiple_account_withdrawals() {
        // Test a single transaction that withdraws from multiple accounts
        let balance_read = Arc::new(SimpleMockBalanceRead::new(BTreeMap::from([
            (ObjectID::from_hex_literal("0x1").unwrap(), 100),
            (ObjectID::from_hex_literal("0x2").unwrap(), 50),
            (ObjectID::from_hex_literal("0x3").unwrap(), 75),
        ])));
        let scheduler = EagerBalanceWithdrawScheduler::new(
            balance_read.clone(),
            SequenceNumber::from_u64(0),
        );

        let account1 = ObjectID::from_hex_literal("0x1").unwrap();
        let account2 = ObjectID::from_hex_literal("0x2").unwrap();
        let account3 = ObjectID::from_hex_literal("0x3").unwrap();

        // Transaction needs: 80 from account1, 60 from account2, 50 from account3
        // Available: 100, 50, 75 - so account2 is insufficient
        let tx = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([
                (account1, Reservation::MaxAmountU64(80)),
                (account2, Reservation::MaxAmountU64(60)), // This will cause pending
                (account3, Reservation::MaxAmountU64(50)),
            ]),
        };
        let (sender, mut receiver) = oneshot::channel();

        scheduler.schedule_withdraws(WithdrawReservations {
            accumulator_version: SequenceNumber::from_u64(1),
            withdraws: vec![tx],
            senders: vec![sender],
        }).await;

        // Should be pending due to account2
        assert!(receiver.try_recv().is_err());

        // Deposit to account2
        balance_read.balances.lock().unwrap().insert(account2, 70);
        scheduler.settle_balances(BalanceSettlement {
            accumulator_version: SequenceNumber::from_u64(1),
            balance_changes: BTreeMap::from([(account2, 20)]), // +20 deposit
        }).await;

        // Now all accounts have sufficient balance
        assert_eq!(receiver.await.unwrap().status, ScheduleStatus::SufficientBalance);
    }

    #[tokio::test]
    async fn test_pending_with_entire_balance_blocking() {
        // Test that EntireBalance reservation blocks other transactions
        let balance_read = Arc::new(SimpleMockBalanceRead::new(BTreeMap::from([
            (ObjectID::from_hex_literal("0x1").unwrap(), 100),
        ])));
        let scheduler = EagerBalanceWithdrawScheduler::new(
            balance_read.clone(),
            SequenceNumber::from_u64(0),
        );

        let account = ObjectID::from_hex_literal("0x1").unwrap();

        // First transaction: reserve entire balance
        let tx1 = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::EntireBalance)]),
        };
        let (sender1, receiver1) = oneshot::channel();
        scheduler.schedule_withdraws(WithdrawReservations {
            accumulator_version: SequenceNumber::from_u64(1),
            withdraws: vec![tx1],
            senders: vec![sender1],
        }).await;
        assert_eq!(receiver1.await.unwrap().status, ScheduleStatus::SufficientBalance);

        // Second transaction: try to withdraw any amount (should be pending)
        let tx2 = TxBalanceWithdraw {
            tx_digest: TransactionDigest::random(),
            reservations: BTreeMap::from([(account, Reservation::MaxAmountU64(10))]),
        };
        let (sender2, mut receiver2) = oneshot::channel();
        scheduler.schedule_withdraws(WithdrawReservations {
            accumulator_version: SequenceNumber::from_u64(2),
            withdraws: vec![tx2],
            senders: vec![sender2],
        }).await;

        // Should be pending because entire balance is reserved
        assert!(receiver2.try_recv().is_err());

        // Settle with the entire balance withdrawn
        balance_read.balances.lock().unwrap().insert(account, 0);
        scheduler.settle_balances(BalanceSettlement {
            accumulator_version: SequenceNumber::from_u64(1),
            balance_changes: BTreeMap::from([(account, -100)]),
        }).await;

        // Version 2 should still be pending
        assert!(receiver2.try_recv().is_err());

        // Settle version 2 - still no balance
        scheduler.settle_balances(BalanceSettlement {
            accumulator_version: SequenceNumber::from_u64(2),
            balance_changes: BTreeMap::new(),
        }).await;

        // Should fail due to insufficient balance
        assert_eq!(receiver2.await.unwrap().status, ScheduleStatus::InsufficientBalance);
    }
}
