// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_universe::{run_and_assert_universe, AUTransactionGen, AccountUniverseGen},
    executor::Executor,
};

/// Run transactions with the given reference gas price.
pub fn run_rgp(
    universe: AccountUniverseGen,
    transaction_gens: Vec<impl AUTransactionGen + Clone>,
    rgp: u64,
) {
    let mut executor = Executor::new_with_rgp(rgp);
    assert!(run_and_assert_universe(universe, transaction_gens, &mut executor).is_ok());
}

// TODO: add other protocol config fuzzers here
