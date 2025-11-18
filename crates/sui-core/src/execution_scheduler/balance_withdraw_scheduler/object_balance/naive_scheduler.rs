// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use parking_lot::Mutex;
use sui_types::{
    accumulator_root::AccumulatorObjId, base_types::SequenceNumber,
    execution_params::BalanceWithdrawStatus,
};
use tokio::sync::{oneshot, watch};

use crate::accumulators::balance_read::AccountBalanceRead;

#[derive(Clone)]
pub(crate) struct ObjectBalanceWithdrawScheduler {
    balance_read: Arc<dyn AccountBalanceRead>,
    inner_state: Arc<Mutex<InnerState>>,
    accumulator_version_sender: Arc<watch::Sender<SequenceNumber>>,
    // We must keep a receiver alive to make sure sends go through and can update the last settled version.
    accumulator_version_receiver: Arc<watch::Receiver<SequenceNumber>>,
}

struct InnerState {
    /// Tracks for each accumulator version, the object accounts that have unsettled withdraws.
    tracked_accounts: BTreeMap<SequenceNumber, BTreeSet<AccumulatorObjId>>,
    /// Tracks for each object account that has unsettled withdraws, the total amount of withdraws
    /// that have not been settled yet, and the amount of withdraws that have not been settled
    /// for each accumulator version.
    unsettled_withdraws: BTreeMap<AccumulatorObjId, UnsettledWithdraws>,
}

#[derive(Default)]
struct UnsettledWithdraws {
    total_amount: u128,
    unsettled_withdraw_map: BTreeMap<SequenceNumber, u128>,
}

pub(crate) enum ObjectBalanceWithdrawStatus {
    SufficientBalance,
    Pending(oneshot::Receiver<BalanceWithdrawStatus>),
}

impl ObjectBalanceWithdrawScheduler {
    pub fn new(
        balance_read: Arc<dyn AccountBalanceRead>,
        starting_accumulator_version: SequenceNumber,
    ) -> Self {
        let (accumulator_version_sender, accumulator_version_receiver) =
            watch::channel(starting_accumulator_version);
        Self {
            balance_read,
            inner_state: Arc::new(Mutex::new(InnerState {
                tracked_accounts: BTreeMap::new(),
                unsettled_withdraws: BTreeMap::new(),
            })),
            accumulator_version_sender: Arc::new(accumulator_version_sender),
            accumulator_version_receiver: Arc::new(accumulator_version_receiver),
        }
    }

    pub fn schedule(
        &self,
        object_withdraws: BTreeMap<AccumulatorObjId, u64>,
        accumulator_version: SequenceNumber,
    ) -> ObjectBalanceWithdrawStatus {
        let last_settled_version = *self.accumulator_version_receiver.borrow();
        // This function is called during execution, which means this transaction is not committed yet,
        // so the settlement transaction at the end of the same consensus commit cannot have settled yet.
        assert!(accumulator_version >= last_settled_version);
        // TODO: Should we use last_settled_version or accumulator_version here?
        if self.check_balance_sufficient(&object_withdraws, accumulator_version) {
            self.record_withdraws(&object_withdraws, accumulator_version);
            return ObjectBalanceWithdrawStatus::SufficientBalance;
        }
        if accumulator_version == last_settled_version {
            return Self::return_insufficient_balance();
        }

        // Spawn a task to wait for the last settled version to become accumulator_version,
        // before we could check again.
        let scheduler = self.clone();
        let accumulator_version_sender = self.accumulator_version_sender.clone();
        let (sender, receiver) = oneshot::channel();
        tokio::spawn(async move {
            let mut version_receiver = accumulator_version_sender.subscribe();
            version_receiver
                .wait_for(|v| *v == accumulator_version)
                .await
                .unwrap();
            // TODO: Is this unwrap safe?

            if scheduler.check_balance_sufficient(&object_withdraws, accumulator_version) {
                // TODO: Is this unwrap safe?
                sender.send(BalanceWithdrawStatus::Unknown).unwrap();
            } else {
                // TODO: Is this unwrap safe?
                sender
                    .send(BalanceWithdrawStatus::InsufficientBalance)
                    .unwrap();
            }
        });
        ObjectBalanceWithdrawStatus::Pending(receiver)
    }

    pub fn settle_accumulator_version(&self, next_accumulator_version: SequenceNumber) {
        let mut inner_state = self.inner_state.lock();
        while let Some(version) = inner_state.tracked_accounts.keys().next().copied() {
            let accounts = if version < next_accumulator_version {
                inner_state.tracked_accounts.remove(&version).unwrap()
            } else {
                break;
            };
            for obj_id in accounts {
                let unsettled = inner_state.unsettled_withdraws.get_mut(&obj_id).unwrap();
                let version_amount = unsettled.unsettled_withdraw_map.remove(&version).unwrap();
                unsettled.total_amount -= version_amount;
            }
        }

        // TODO: Is this unwrap safe?
        self.accumulator_version_sender
            .send(next_accumulator_version)
            .unwrap();
    }

    fn check_balance_sufficient(
        &self,
        object_withdraws: &BTreeMap<AccumulatorObjId, u64>,
        accumulator_version: SequenceNumber,
    ) -> bool {
        let inner_state = self.inner_state.lock();
        for (obj_id, amount) in object_withdraws {
            let balance = self
                .balance_read
                .get_account_balance(obj_id, accumulator_version);
            let unsettled = inner_state
                .unsettled_withdraws
                .get(obj_id)
                .map(|withdraws| withdraws.total_amount)
                .unwrap_or_default();
            println!("balance: {}, unsettled: {}", balance, unsettled);
            assert!(balance >= unsettled);
            if balance - unsettled < *amount as u128 {
                return false;
            }
        }
        true
    }

    fn record_withdraws(
        &self,
        object_withdraws: &BTreeMap<AccumulatorObjId, u64>,
        accumulator_version: SequenceNumber,
    ) {
        let mut inner_state = self.inner_state.lock();
        for (obj_id, amount) in object_withdraws {
            inner_state
                .tracked_accounts
                .entry(accumulator_version)
                .or_default()
                .insert(*obj_id);

            let unsettled = inner_state.unsettled_withdraws.entry(*obj_id).or_default();
            unsettled.total_amount += *amount as u128;
            let version_entry = unsettled
                .unsettled_withdraw_map
                .entry(accumulator_version)
                .or_default();
            *version_entry += *amount as u128;
        }
    }

    fn return_insufficient_balance() -> ObjectBalanceWithdrawStatus {
        let (sender, receiver) = oneshot::channel();
        // TODO: Is this unwrap safe?
        sender
            .send(BalanceWithdrawStatus::InsufficientBalance)
            .unwrap();
        ObjectBalanceWithdrawStatus::Pending(receiver)
    }

    #[cfg(test)]
    pub fn get_unsettled_withdraw_amount(&self, obj_id: &AccumulatorObjId) -> u128 {
        let inner_state = self.inner_state.lock();
        inner_state
            .unsettled_withdraws
            .get(obj_id)
            .map(|withdraws| withdraws.total_amount)
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub fn get_tracked_versions(&self) -> BTreeSet<SequenceNumber> {
        let inner_state = self.inner_state.lock();
        inner_state.tracked_accounts.keys().copied().collect()
    }

    #[cfg(test)]
    pub fn get_tracked_accounts(&self, version: SequenceNumber) -> BTreeSet<AccumulatorObjId> {
        let inner_state = self.inner_state.lock();
        inner_state
            .tracked_accounts
            .get(&version)
            .map(|accounts| accounts.iter().copied().collect())
            .unwrap_or_default()
    }
}
