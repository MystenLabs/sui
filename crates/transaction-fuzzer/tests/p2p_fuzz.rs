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
            transfers in vec(any_with::<P2PTransferGen>((1_000_000, 100_000_000)), 0..default_num_transactions()),
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
            transfers in vec(any_with::<P2PTransferGen>((1, 10_000)), 0..default_num_transactions()),
        ) {
        run_and_assert_universe(universe, transfers).unwrap();
    }

}
