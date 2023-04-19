// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use proptest::arbitrary::*;
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::proptest;
use transaction_fuzzer::account_universe::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(15))]

    #[test]
    #[cfg_attr(msim, ignore)]
    fn fuzz_p2p_low_balance(
        universe in AccountUniverseGen::strategy(
            2..default_num_accounts(),
            1_000_000u64..10_000_000,
            ),
            transfers in vec(any_with::<P2PTransferGenGoodGas>((1_000_000, 100_000_000)), 0..default_num_transactions()),
        ) {
        run_and_assert_universe(universe, transfers).unwrap();
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn fuzz_p2p_high_balance(
        universe in AccountUniverseGen::strategy(
            2..default_num_accounts(),
            1_000_000_000_000u64..10_000_000_000_000,
            ),
            transfers in vec(any_with::<P2PTransferGenGoodGas>((1, 10_000)), 0..default_num_transactions()),
        ) {
        run_and_assert_universe(universe, transfers).unwrap();
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn fuzz_p2p_random_gas_budget_high_balance(
        universe in AccountUniverseGen::strategy(
            2..default_num_accounts(),
            1_000_000_000_000u64..10_000_000_000_000,
            ),
            transfers in vec(any_with::<P2PTransferGenRandomGas>((1, 10_000)), 0..default_num_transactions()),
        ) {
        run_and_assert_universe(universe, transfers).unwrap();
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn fuzz_p2p_random_gas_budget_low_balance(
        universe in AccountUniverseGen::strategy(
            2..default_num_accounts(),
            1_000_000u64..10_000_000,
            ),
            transfers in vec(any_with::<P2PTransferGenRandomGas>((1_000_000, 100_000_000)), 0..default_num_transactions()),
        ) {
        run_and_assert_universe(universe, transfers).unwrap();
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn fuzz_p2p_random_gas_budget_and_price_high_balance(
        universe in AccountUniverseGen::strategy(
            2..default_num_accounts(),
            1_000_000_000_000u64..10_000_000_000_000,
            ),
            transfers in vec(any_with::<P2PTransferGenRandomGasRandomPrice>((1, 10_000)), 0..default_num_transactions()),
        ) {
        run_and_assert_universe(universe, transfers).unwrap();
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn fuzz_p2p_random_gas_budget_and_price_low_balance(
        universe in AccountUniverseGen::strategy(
            2..default_num_accounts(),
            1_000_000u64..10_000_000,
            ),
            transfers in vec(any_with::<P2PTransferGenRandomGasRandomPrice>((1_000_000, 100_000_000)), 0..default_num_transactions()),
        ) {
        run_and_assert_universe(universe, transfers).unwrap();
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn fuzz_p2p_mixed(
        universe in AccountUniverseGen::strategy(
            2..default_num_accounts(),
            log_balance_strategy(1_000_000, 1_000_000_000_000),
            ),
            transfers in vec(p2p_transfer_strategy(1, 1_000_000), 0..default_num_transactions()),
        ) {
        run_and_assert_universe(universe, transfers).unwrap();
    }
}
