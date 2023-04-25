// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::executor::{ExecutionResult, Executor};
use once_cell::sync::Lazy;
use proptest::{prelude::*, strategy::Union};
use std::{fmt, sync::Arc};
use sui_types::{messages::VerifiedTransaction, storage::ObjectStore};

mod account;
mod helpers;
mod transfer_gen;
mod universe;
pub use account::*;
pub use transfer_gen::*;
pub use universe::*;

static UNIVERSE_SIZE: Lazy<usize> = Lazy::new(|| {
    use std::env;

    match env::var("UNIVERSE_SIZE") {
        Ok(s) => match s.parse::<usize>() {
            Ok(val) => val,
            Err(err) => {
                panic!("Could not parse universe size, aborting: {:?}", err);
            }
        },
        Err(env::VarError::NotPresent) => 30,
        Err(err) => {
            panic!(
                "Could not read universe size from the environment, aborting: {:?}",
                err
            );
        }
    }
});

pub fn default_num_accounts() -> usize {
    *UNIVERSE_SIZE
}

pub fn default_num_transactions() -> usize {
    *UNIVERSE_SIZE * 2
}

/// Represents any sort of transaction that can be done in an account universe.
pub trait AUTransactionGen: fmt::Debug {
    /// Applies this transaction onto the universe, updating balances within the universe as
    /// necessary. Returns a signed transaction that can be run on the VM and the the execution status.
    fn apply(
        &self,
        universe: &mut AccountUniverse,
        exec: &mut Executor,
    ) -> (VerifiedTransaction, ExecutionResult);

    /// Creates an arced version of this transaction, suitable for dynamic dispatch.
    fn arced(self) -> Arc<dyn AUTransactionGen>
    where
        Self: 'static + Sized,
    {
        Arc::new(self)
    }
}

impl AUTransactionGen for Arc<dyn AUTransactionGen> {
    fn apply(
        &self,
        universe: &mut AccountUniverse,
        exec: &mut Executor,
    ) -> (VerifiedTransaction, ExecutionResult) {
        (**self).apply(universe, exec)
    }
}

/// Returns a [`Strategy`] that provides a variety of balances (or transfer amounts) over a roughly
/// logarithmic distribution.
pub fn log_balance_strategy(min_balance: u64, max_balance: u64) -> impl Strategy<Value = u64> {
    // The logarithmic distribution is modeled by uniformly picking from ranges of powers of 2.
    assert!(max_balance >= min_balance, "minimum to make sense");
    let mut strategies = vec![];
    // Balances below and around the minimum are interesting but don't cover *every* power of 2,
    // just those starting from the minimum.
    let mut lower_bound: u64 = 0;
    let mut upper_bound: u64 = min_balance;
    loop {
        strategies.push(lower_bound..upper_bound);
        if upper_bound >= max_balance {
            break;
        }
        lower_bound = upper_bound;
        upper_bound = (upper_bound * 2).min(max_balance);
    }
    Union::new(strategies)
}

/// Run these transactions and verify the expected output.
pub fn run_and_assert_universe(
    universe: AccountUniverseGen,
    transaction_gens: Vec<impl AUTransactionGen + Clone>,
    executor: &mut Executor,
) -> Result<(), TestCaseError> {
    let mut universe = universe.setup(executor);
    let (transactions, expected_values): (Vec<_>, Vec<_>) = transaction_gens
        .iter()
        .map(|transaction_gen| transaction_gen.clone().apply(&mut universe, executor))
        .unzip();
    let outputs = executor.execute_transactions(transactions);
    prop_assert_eq!(outputs.len(), expected_values.len());

    for (idx, (output, expected)) in outputs.iter().zip(&expected_values).enumerate() {
        prop_assert!(
            output == expected,
            "unexpected status for transaction {} expected {:#?} but got {:#?}",
            idx,
            expected,
            output
        );
    }
    assert_accounts_match(&universe, executor)
}

pub fn assert_accounts_match(
    universe: &AccountUniverse,
    executor: &Executor,
) -> Result<(), TestCaseError> {
    for (idx, account) in universe.accounts().iter().enumerate() {
        for (balance_idx, acc_object) in account.current_coins.iter().enumerate() {
            let object = executor
                .state
                .db()
                .get_object(&acc_object.id())
                .unwrap()
                .unwrap();
            let total_sui_value =
                object.get_total_sui(&executor.state.db()).unwrap() - object.storage_rebate;
            let account_balance_i = account.current_balances[balance_idx];
            prop_assert_eq!(
                account_balance_i,
                total_sui_value,
                "account {} should have correct balance {} for object {} but got {}",
                idx,
                total_sui_value,
                acc_object.id(),
                account_balance_i
            );
        }
    }
    Ok(())
}
