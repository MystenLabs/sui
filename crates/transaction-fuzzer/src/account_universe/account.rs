// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use proptest::prelude::*;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
    object::Object,
};

use crate::executor::Executor;

pub const INITIAL_BALANCE: u64 = 10_000_000_000;
pub const NUM_GAS_OBJECTS: usize = 1;

#[derive(Debug)]
pub struct Account {
    pub address: SuiAddress,
    pub key: AccountKeyPair,
}

// `Arc` account since the key pair is non-copyable
#[derive(Debug, Clone)]
pub struct AccountData {
    pub account: Arc<Account>,
    pub coins: Vec<Object>,
    pub initial_balances: Vec<u64>,
    pub balance_creation_amt: u64,
}

#[derive(Clone, Debug)]
pub struct AccountCurrent {
    pub initial_data: AccountData,
    pub current_balances: Vec<u64>,
    pub current_coins: Vec<Object>,
    // Non-coin objects
    pub current_objects: Vec<ObjectID>,
}

impl Account {
    pub fn new_random() -> Self {
        let (address, key) = get_key_pair();
        Self { address, key }
    }
}

impl AccountData {
    pub fn new_random() -> Self {
        let account = Account::new_random();
        Self::new_with_account_and_balance(Arc::new(account), INITIAL_BALANCE)
    }

    pub fn new_with_account_and_balance(account: Arc<Account>, initial_balance: u64) -> Self {
        let coins = (0..NUM_GAS_OBJECTS)
            .map(|_| {
                let gas_object_id = ObjectID::random();
                Object::with_id_owner_gas_for_testing(
                    gas_object_id,
                    account.address,
                    initial_balance,
                )
            })
            .collect();
        let initial_balances = (0..NUM_GAS_OBJECTS).map(|_| initial_balance).collect();
        Self {
            account,
            coins,
            initial_balances,
            balance_creation_amt: initial_balance,
        }
    }
}

impl AccountCurrent {
    pub fn new(account: AccountData) -> Self {
        Self {
            current_balances: account.initial_balances.clone(),
            current_coins: account.coins.clone(),
            current_objects: vec![],
            initial_data: account,
        }
    }

    // TODO: Use this to get around the fact that we need to update object refs in the
    // executor..figure out a better way to do this other than just creating a gas object for each
    // transaction.
    pub fn new_gas_object(&mut self, exec: &mut Executor) -> Object {
        // We just create a new gas object for this transaction
        let gas_object_id = ObjectID::random();
        let gas_object = Object::with_id_owner_gas_for_testing(
            gas_object_id,
            self.initial_data.account.address,
            self.initial_data.balance_creation_amt,
        );
        exec.add_object(gas_object.clone());
        self.current_balances
            .push(self.initial_data.balance_creation_amt);
        self.current_coins.push(gas_object.clone());
        gas_object
    }
}

impl Arbitrary for Account {
    type Parameters = ();
    type Strategy = fn() -> Account;
    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        Account::new_random as Self::Strategy
    }
}

impl AccountData {
    /// Returns a [`Strategy`] that creates `AccountData` instances.
    pub fn strategy(balance_strategy: impl Strategy<Value = u64>) -> impl Strategy<Value = Self> {
        (any::<Account>(), balance_strategy).prop_map(|(account, balance)| {
            AccountData::new_with_account_and_balance(Arc::new(account), balance)
        })
    }
}
