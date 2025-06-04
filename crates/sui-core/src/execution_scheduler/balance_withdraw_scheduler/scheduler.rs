// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This modules implements an optimistic scheduler for balance withdraw reservations.
//! A transaction that contains withdraw reservations when enqueued through execution scheduler,
//! will first be enqueued here and wait for notifications.
//! Only after the withdraw reservations are secured it will then proceed to
//! check input object dependencies and eventually proceed to execution.
//! Note that when checking input object dependencies, a transaction no longer
//! needs to check the dependencies on the accumulator root object, because that dependency
//! is managed by this module.
//!
//! When a transaction is enqueued here, a conservative way to process it would be to
//! wait until the dependent accumulator version is fully settled when the settlement
//! transaction is executed. At that point we must have reached a deterministic state
//! where we know for sure if there is enough balance for the withdraw reservations.
//! However, that would significantly limit the throughput of the system.
//!
//! Instead, we use an optimistic approach where we track the guaranteed minimum balance
//! for each account, by using a recent settled balance in this account, together with all
//! pending withdraws that have been reserved for this account.
//!
//! When a transaction is enqueued here, we collect all the withdraw reservations
//! and their associated accounts.
//! We immediately try to see that for each account, whether we can guarantee it
//! will have enough balance to satisfy the withdraw reservation.
//! Similarly, whenever a settlement transaction is executed, we update the guaranteed minimum balance
//! for each account, and try to reserve the withdraws for each account again.
//!
//! In either case if we know for sure that all withdraw reservations in a transaction
//! can be satisfied, we send a notification to the transaction so it can
//! proceed to execution without waiting for the accumulator version to be fully settled.
//!
//! The implementation contains two critical entry points:
//! 1. When a transaction is enqueued, we collect all the withdraw reservations and their associated accounts.
//!    We then try to reserve the withdraws for each account.
//! 2. When a settlement transaction is executed, we update the guaranteed minimum balance for each account,
//!    and try to reserve the withdraws for each account again.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use dashmap::DashMap;
use mysten_common::debug_fatal;
use mysten_metrics::monitored_mpsc::{self, UnboundedReceiver, UnboundedSender};
use parking_lot::Mutex;
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
};
use tokio::sync::watch;

use crate::execution_scheduler::balance_withdraw_scheduler::{
    account_state::{AccountState, TxWithdrawRegistration},
    balance_read::AccountBalanceRead,
    BalanceSettlement, ScheduleResult, TxBalanceWithdraw, WithdrawReservations,
};

#[allow(dead_code)]
#[derive(Clone)]
struct SettlementState {
    /// The last settled accumulator version.
    last_known_settled_version: SequenceNumber,
    /// For each accumulator version, the set of account IDs that have scheduled withdraw reservations
    /// at that accumulator version, but have not yet been settled.
    pending_settlements: BTreeMap<SequenceNumber, BTreeSet<ObjectID>>,
}

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct WithdrawScheduler {
    balance_read: Arc<dyn AccountBalanceRead>,
    /// All accounts that we need to track the status.
    /// These are accounts that have scheduled withdraw reservations,
    /// and have not yet been settled.
    account_states: Arc<DashMap<ObjectID, AccountState>>,
    settlement_state: Arc<Mutex<SettlementState>>,
    /// Channel that allows us to receive and process balance settlements asynchronously.
    settlement_sender: UnboundedSender<BalanceSettlement>,
    /// Channel that allows us to receive and process withdraw reservations asynchronously.
    reservation_sender: UnboundedSender<WithdrawReservations>,
}

impl WithdrawScheduler {
    #[allow(dead_code)]
    pub fn new(balance_read: Arc<dyn AccountBalanceRead>) -> Arc<Self> {
        let (settlement_sender, settlement_receiver) =
            monitored_mpsc::unbounded_channel("balance_withdraw_scheduler_settlement");
        let (reservation_sender, reservation_receiver) =
            monitored_mpsc::unbounded_channel("balance_withdraw_scheduler_reservation");
        let cur_accumulator_version = balance_read.get_accumulator_version();
        let scheduler = Arc::new(Self {
            balance_read,
            account_states: Arc::new(DashMap::new()),
            settlement_state: Arc::new(Mutex::new(SettlementState {
                last_known_settled_version: cur_accumulator_version,
                pending_settlements: BTreeMap::new(),
            })),
            settlement_sender,
            reservation_sender,
        });
        let expected_version = cur_accumulator_version.next();
        let scheduler_clone = scheduler.clone();
        // TODO: Use monitored tasks?
        tokio::spawn(async move {
            scheduler_clone
                .process_settlement_task(settlement_receiver, expected_version)
                .await;
        });
        let scheduler_clone = scheduler.clone();
        tokio::spawn(async move {
            scheduler_clone
                .process_withdraw_reservation_task(reservation_receiver, expected_version)
                .await;
        });
        scheduler
    }

    #[allow(dead_code)]
    pub fn settle_balance(&self, settlement: BalanceSettlement) {
        if let Err(err) = self.settlement_sender.send(settlement) {
            tracing::error!("Failed to send balance settlement: {:?}", err);
        }
    }

    async fn process_settlement_task(
        self: Arc<Self>,
        mut settlement_receiver: UnboundedReceiver<BalanceSettlement>,
        mut expected_version: SequenceNumber,
    ) {
        let mut pending_settlements = BTreeMap::new();
        while let Some(settlement) = settlement_receiver.recv().await {
            pending_settlements.insert(settlement.accumulator_version, settlement);
            while let Some(settlement) = pending_settlements.remove(&expected_version) {
                expected_version = settlement.accumulator_version.next();
                self.process_settlement(settlement);
            }
        }
    }

    fn process_settlement(&self, settlement: BalanceSettlement) {
        let unique_accounts = {
            let mut guard = self.settlement_state.lock();
            let previous_version = guard.last_known_settled_version;
            assert_eq!(previous_version.next(), settlement.accumulator_version);
            guard.last_known_settled_version = settlement.accumulator_version;

            let mut unique_accounts = settlement
                .balance_changes
                .keys()
                .copied()
                .collect::<BTreeSet<_>>();
            if let Some(pending) = guard.pending_settlements.remove(&previous_version) {
                unique_accounts.extend(pending);
            }
            for account_id in unique_accounts.iter() {
                if let Some(mut account_state) = self.account_states.get_mut(account_id) {
                    account_state.settle_accumulator_version(
                        previous_version,
                        settlement
                            .balance_changes
                            .get(account_id)
                            .copied()
                            // If the settlement does not contain a balance change for this account,
                            // we use 0 as the balance change.
                            .unwrap_or(0),
                    );
                } else if !settlement.balance_changes.contains_key(account_id) {
                    debug_fatal!(
                        "Account {} was in pending_settlements at version {}, but is not tracked in the scheduler",
                        account_id,
                        previous_version,
                    );
                }
            }
            if let Some(pending) = guard
                .pending_settlements
                .get(&settlement.accumulator_version)
            {
                unique_accounts.extend(pending.iter().copied());
            }
            unique_accounts
        };

        for account_id in unique_accounts {
            self.try_reserve_withdraws_for_account(account_id, settlement.accumulator_version);
        }
    }

    #[allow(dead_code)]
    pub fn schedule_withdraw_reservation(
        &self,
        accumulator_version: SequenceNumber,
        withdraws: Vec<TxBalanceWithdraw>,
    ) -> BTreeMap<TransactionDigest, watch::Receiver<ScheduleResult>> {
        let (reservations, receivers) = WithdrawReservations::new(accumulator_version, withdraws);
        if let Err(err) = self.reservation_sender.send(reservations) {
            tracing::error!("Failed to send withdraw reservations: {:?}", err);
        }
        receivers
    }

    async fn process_withdraw_reservation_task(
        self: Arc<Self>,
        mut reservation_receiver: UnboundedReceiver<WithdrawReservations>,
        mut expected_version: SequenceNumber,
    ) {
        let mut pending_reservations = BTreeMap::new();
        while let Some(event) = reservation_receiver.recv().await {
            if event.accumulator_version < expected_version {
                // It is possible to receive withdraw reservations for the same accumulator version
                // multiple times due to the race between consensus and checkpoint execution.
                // Hence we may receive a version from the past after the version is updated.
                for sender in event.senders {
                    let _ = sender.send(ScheduleResult::AlreadyScheduled);
                }
                continue;
            }
            pending_reservations.insert(event.accumulator_version, event);
            while let Some(event) = pending_reservations.remove(&expected_version) {
                expected_version = event.accumulator_version.next();
                self.process_withdraw_reservation(event);
            }
        }
    }

    fn process_withdraw_reservation(&self, reservations: WithdrawReservations) {
        let WithdrawReservations {
            accumulator_version,
            withdraws,
            senders,
        } = reservations;
        let (last_known_settled_version, unique_accounts) = {
            let mut guard = self.settlement_state.lock();
            let last_known_settled_version = guard.last_known_settled_version;
            if last_known_settled_version > accumulator_version {
                for sender in senders {
                    let _ = sender.send(ScheduleResult::ReadyForExecution);
                }
                return;
            }

            let mut all_registrations: BTreeMap<_, Vec<_>> = BTreeMap::new();
            for (withdraw, sender) in withdraws.into_iter().zip(senders) {
                let registration = TxWithdrawRegistration::new(withdraw.reservations, sender);
                for (account_id, _) in registration.pending_accounts.read().iter() {
                    all_registrations
                        .entry(*account_id)
                        .or_default()
                        .push(registration.clone());
                }
            }
            let unique_accounts: Vec<_> = all_registrations.keys().copied().collect();

            let pending_accounts = guard
                .pending_settlements
                .entry(accumulator_version)
                .or_default();
            assert!(pending_accounts.is_empty());
            pending_accounts.extend(unique_accounts.clone());

            for (account_id, registrations) in all_registrations {
                self.account_states
                    .entry(account_id)
                    .or_insert_with(|| {
                        AccountState::new(
                            self.balance_read.as_ref(),
                            &account_id,
                            last_known_settled_version,
                        )
                    })
                    .add_registrations(accumulator_version, registrations);
            }
            (last_known_settled_version, unique_accounts)
            // guard is dropped here.
        };
        for account_id in unique_accounts {
            self.try_reserve_withdraws_for_account(account_id, last_known_settled_version);
        }
    }

    fn try_reserve_withdraws_for_account(
        &self,
        account_id: ObjectID,
        last_known_settled_version: SequenceNumber,
    ) -> Vec<Arc<TxWithdrawRegistration>> {
        let scheduled_registrations =
            if let Some(mut account_state) = self.account_states.get_mut(&account_id) {
                account_state.try_schedule_reservations(&account_id, last_known_settled_version)
            } else {
                Vec::new()
            };
        self.account_states
            .remove_if(&account_id, |_, account_state| account_state.is_empty());
        scheduled_registrations
    }
}
