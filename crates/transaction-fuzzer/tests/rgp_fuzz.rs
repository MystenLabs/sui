// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::proptest;
use transaction_fuzzer::account_universe::default_num_accounts;
use transaction_fuzzer::account_universe::default_num_transactions;
use transaction_fuzzer::account_universe::AccountUniverseGen;
use transaction_fuzzer::account_universe::P2PTransferGenGasPriceInRange;
use transaction_fuzzer::config_fuzzer::run_rgp;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]
    #[test]
    #[cfg_attr(msim, ignore)]
    fn fuzz_low_rgp_low_gas_price(
        universe in AccountUniverseGen::strategy(3..default_num_accounts(), 1_000_000_000u64..10_000_000_000),
        transfers in vec(any_with::<P2PTransferGenGasPriceInRange>((0u64, 10_000)), 0..default_num_transactions()),
        rgp in 0u64..10_000u64,
    ) {
        run_rgp(universe, transfers, rgp);
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn fuzz_high_rgp_high_gas_price(
        universe in AccountUniverseGen::strategy(3..default_num_accounts(), 1_000_000_000u64..10_000_000_000),
        transfers in vec(any_with::<P2PTransferGenGasPriceInRange>((10_000u64, 100_000u64)), 0..default_num_transactions()),
        rgp in 10_000u64..100_000u64,
    ) {
        run_rgp(universe, transfers, rgp);
    }
}
