// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use proptest::arbitrary::*;
use proptest::collection::vec;
use proptest::prelude::*;
use transaction_fuzzer::account_universe::*;
use transaction_fuzzer::run_proptest;

const NUM_RUNS: u32 = 20;

#[test]
#[cfg_attr(msim, ignore)]
fn fuzz_p2p_low_balance() {
    let universe =
        AccountUniverseGen::strategy(3..default_num_accounts(), 1_000_000u64..10_000_000);
    let transfers = vec(
        any_with::<P2PTransferGenGoodGas>((1_000_000, 100_000_000)),
        0..default_num_transactions(),
    );
    let strategy = (universe, transfers).boxed();
    run_proptest(NUM_RUNS, strategy, |(universe, transfers), mut executor| {
        run_and_assert_universe(universe, transfers, &mut executor)
    });
}

#[test]
#[cfg_attr(msim, ignore)]
fn fuzz_p2p_high_balance() {
    let universe = AccountUniverseGen::strategy(
        3..default_num_accounts(),
        1_000_000_000_000u64..10_000_000_000_000,
    );
    let transfers = vec(
        any_with::<P2PTransferGenGoodGas>((1, 10_000)),
        0..default_num_transactions(),
    );
    let strategy = (universe, transfers).boxed();
    run_proptest(NUM_RUNS, strategy, |(universe, transfers), mut executor| {
        run_and_assert_universe(universe, transfers, &mut executor)
    });
}

#[test]
#[cfg_attr(msim, ignore)]
fn fuzz_p2p_random_gas_budget_high_balance() {
    let universe = AccountUniverseGen::strategy(
        3..default_num_accounts(),
        1_000_000_000_000u64..10_000_000_000_000,
    );
    let transfers = vec(
        any_with::<P2PTransferGenRandomGas>((1, 10_000)),
        0..default_num_transactions(),
    );
    let strategy = (universe, transfers).boxed();
    run_proptest(NUM_RUNS, strategy, |(universe, transfers), mut executor| {
        run_and_assert_universe(universe, transfers, &mut executor)
    });
}

#[test]
#[cfg_attr(msim, ignore)]
fn fuzz_p2p_random_gas_budget_low_balance() {
    let universe =
        AccountUniverseGen::strategy(3..default_num_accounts(), 1_000_000u64..10_000_000);
    let transfers = vec(
        any_with::<P2PTransferGenRandomGas>((1_000_000, 100_000_000)),
        0..default_num_transactions(),
    );
    let strategy = (universe, transfers).boxed();
    run_proptest(NUM_RUNS, strategy, |(universe, transfers), mut executor| {
        run_and_assert_universe(universe, transfers, &mut executor)
    });
}

#[test]
#[cfg_attr(msim, ignore)]
fn fuzz_p2p_random_gas_budget_and_price_high_balance() {
    let universe = AccountUniverseGen::strategy(
        3..default_num_accounts(),
        1_000_000_000_000u64..10_000_000_000_000,
    );
    let transfers = vec(
        any_with::<P2PTransferGenRandomGasRandomPrice>((1, 10_000)),
        0..default_num_transactions(),
    );
    let strategy = (universe, transfers).boxed();
    run_proptest(NUM_RUNS, strategy, |(universe, transfers), mut executor| {
        run_and_assert_universe(universe, transfers, &mut executor)
    });
}

#[test]
#[cfg_attr(msim, ignore)]
fn fuzz_p2p_random_gas_budget_and_price_low_balance() {
    let universe =
        AccountUniverseGen::strategy(3..default_num_accounts(), 1_000_000u64..10_000_000);
    let transfers = vec(
        any_with::<P2PTransferGenRandomGasRandomPrice>((1_000_000, 100_000_000)),
        0..default_num_transactions(),
    );
    let strategy = (universe, transfers).boxed();
    run_proptest(NUM_RUNS, strategy, |(universe, transfers), mut executor| {
        run_and_assert_universe(universe, transfers, &mut executor)
    });
}

#[test]
#[cfg_attr(msim, ignore)]
fn fuzz_p2p_rand_gas_budget_price_and_coins() {
    let universe = AccountUniverseGen::strategy(
        3..default_num_accounts(),
        10_000_000_000u64..1_000_000_000_000,
    );
    let transfers = vec(
        any_with::<P2PTransferGenRandGasRandPriceRandCoins>((1_000_000, 100_000_000)),
        0..default_num_transactions(),
    );
    let strategy = (universe, transfers).boxed();
    run_proptest(5, strategy, |(universe, transfers), mut executor| {
        run_and_assert_universe(universe, transfers, &mut executor)
    });
}

#[test]
#[cfg_attr(msim, ignore)]
fn fuzz_p2p_random_gas_budget_and_price_high_balance_random_sponsorship() {
    let universe = AccountUniverseGen::strategy(
        3..default_num_accounts(),
        1_000_000_000_000u64..10_000_000_000_000,
    );
    let transfers = vec(
        any_with::<P2PTransferGenRandomGasRandomPriceRandomSponsorship>((1, 10_000)),
        0..default_num_transactions(),
    );
    let strategy = (universe, transfers).boxed();
    run_proptest(
        NUM_RUNS / 2,
        strategy,
        |(universe, transfers), mut executor| {
            run_and_assert_universe(universe, transfers, &mut executor)
        },
    );
}

#[test]
#[cfg_attr(msim, ignore)]
fn fuzz_p2p_random_gas_budget_and_price_low_balance_random_sponsorship() {
    let universe =
        AccountUniverseGen::strategy(3..default_num_accounts(), 1_000_000u64..10_000_000);
    let transfers = vec(
        any_with::<P2PTransferGenRandomGasRandomPriceRandomSponsorship>((1_000_000, 100_000_000)),
        0..default_num_transactions(),
    );
    let strategy = (universe, transfers).boxed();
    run_proptest(
        NUM_RUNS / 2,
        strategy,
        |(universe, transfers), mut executor| {
            run_and_assert_universe(universe, transfers, &mut executor)
        },
    );
}

#[test]
#[cfg_attr(msim, ignore)]
fn fuzz_p2p_mixed() {
    let universe = AccountUniverseGen::strategy(
        3..default_num_accounts(),
        log_balance_strategy(1_000_000, 1_000_000_000_000),
    );
    let transfers = vec(
        p2p_transfer_strategy(1, 1_000_000),
        0..default_num_transactions(),
    );
    let strategy = (universe, transfers).boxed();
    run_proptest(NUM_RUNS, strategy, |(universe, transfers), mut executor| {
        run_and_assert_universe(universe, transfers, &mut executor)
    });
}
