// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only, allow(unused_variable, unused_function)]
module tests::create_pool {
    use sui::sui::SUI;
    use tests::test_runner::{Self as test, USD};

    #[test] fun test_create_pool() {
        let mut test = test::new();
        let ctx = &mut test.next_tx(@alice);
        let (pool, cap) = test.new_pool().into<SUI, USD>(ctx);

        test.destroy(pool).destroy(cap);
    }

    #[test, expected_failure(abort_code = deepbook::clob_v2::EInvalidTickSizeLotSize)]
    fun test_create_pool_invalid_tick_size_fail() {
        let mut test = test::new();
        let ctx = &mut test.next_tx(@alice);
        let (pool, cap) = test.new_pool().tick_size(0).into<USD, SUI>(ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = deepbook::clob_v2::EInvalidTickSizeLotSize)]
    fun test_create_pool_invalid_lot_size_fail() {
        let mut test = test::new();
        let ctx = &mut test.next_tx(@alice);
        let (pool, cap) = test.new_pool().lot_size_unsafe(0).into<USD, SUI>(ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = deepbook::clob_v2::EInvalidPair)]
    fun test_create_pool_invalid_pair() {
        let mut test = test::new();
        let ctx = &mut test.next_tx(@alice);
        let (pool, cap) = test.new_pool().into<USD, USD>(ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = deepbook::clob_v2::EInvalidFeeRateRebateRate)]
    fun test_create_pool_invalid_fee_rate_fail() {
        let mut test = test::new();
        let ctx = &mut test.next_tx(@alice);
        let (pool, cap) = test.new_pool()
            .taker_fee_rate(0)
            .maker_rebate_rate(100) // rebase rate > fee rate - fail
            .into<USD, SUI>(ctx);

        abort 1337
    }
}
