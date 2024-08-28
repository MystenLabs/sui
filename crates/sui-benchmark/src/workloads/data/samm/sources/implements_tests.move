// Copyright 2022 OmniBTC Authors. Licensed under Apache-2.0 License.

#[test_only]
/// Refer to https://github.com/OmniBTC/Aptos-AMM-swap/blob/main/tests/interface_tests.move
///
/// Tests for the pool module.
/// They are sequential and based on top of each other.
/// ```
/// * - test_add_liquidity_with_register
/// |   +-- test_add_liquidity
/// |   +-- test_swap_usdt_for_xbtc
/// |       +-- test_swap_xbtc_for_usdt
/// |           +-- test_withdraw_almost_all
/// |           +-- test_withdraw_all
/// | - test_get_amount_out_does_not_overflow_on_coin_in_close_to_u64_max
/// | - test_add_liquidity_aborts_if_pool_has_full
/// | - test_swap_with_value_should_ok
/// | - test_lp_name
/// | - test_order
/// ```
module swap::implements_tests {
    use std::ascii::into_bytes;
    use std::bcs;
    use std::string::utf8;
    use std::type_name::{get, into_string};
    use std::vector;

    use sui::coin::{mint_for_testing as mint, burn_for_testing as burn};
    use sui::sui::SUI;
    use sui::test_scenario::{Self, Scenario, next_tx, ctx, end};

    use swap::implements::{Self, LP, Global};
    use swap::math::{sqrt, mul_to_u128};

    const XBTC_AMOUNT: u64 = 100000000;
    const USDT_AMOUNT: u64 = 1900000000000;
    const MINIMAL_LIQUIDITY: u64 = 1000;
    const MAX_U64: u64 = 18446744073709551615;

    // test coins

    struct XBTC {}

    struct USDT {}

    struct BEEP {}

    // Tests section
    #[test]
    fun test_lp_name() {
        let expect_name = utf8(
            b"LP-0000000000000000000000000000000000000000000000000000000000000002::sui::SUI-0000000000000000000000000000000000000000000000000000000000000000::implements_tests::BEEP"
        );

        assert!(
            into_bytes(into_string(get<SUI>())) == b"0000000000000000000000000000000000000000000000000000000000000002::sui::SUI",
            1
        );
        assert!(
            into_bytes(into_string(get<BEEP>())) == b"0000000000000000000000000000000000000000000000000000000000000000::implements_tests::BEEP",
            2
        );

        let bcs_sui = bcs::to_bytes(&get<SUI>());
        let bcs_beep = bcs::to_bytes(&get<BEEP>());

        // bcs for vector use ULEB128 encode
        // for this test, the first byte is the length of bcs data
        let length_bcs_sui = vector::borrow(&bcs_sui, 0);
        let length_bcs_beep = vector::borrow(&bcs_beep, 0);

        assert!(*length_bcs_sui < *length_bcs_beep, 3);

        let lp_name = implements::generate_lp_name<SUI, BEEP>();
        assert!(lp_name == expect_name, 4);
        let lp_name = implements::generate_lp_name<BEEP, SUI>();
        assert!(lp_name == expect_name, 5);

        let expect_name = utf8(
            b"LP-0000000000000000000000000000000000000000000000000000000000000000::implements_tests::USDT-0000000000000000000000000000000000000000000000000000000000000000::implements_tests::XBTC"
        );

        let lp_name = implements::generate_lp_name<XBTC, USDT>();
        assert!(lp_name == expect_name, 6);
        let lp_name = implements::generate_lp_name<USDT, XBTC>();
        assert!(lp_name == expect_name, 7);
    }

    #[test]
    fun test_order() {
        assert!(implements::is_order<SUI, BEEP>(), 1);
        assert!(implements::is_order<USDT, XBTC>(), 2);
    }

    #[test]
    fun test_add_liquidity_with_register() {
        let scenario = scenario();
        add_liquidity_with_register(&mut scenario);
        end(scenario);
    }

    #[test]
    fun test_add_liquidity() {
        let scenario = scenario();
        add_liquidity(&mut scenario);
        end(scenario);
    }

    #[test]
    fun test_swap_usdt_for_xbtc() {
        let scenario = scenario();
        swap_usdt_for_xbtc(&mut scenario);
        end(scenario);
    }

    #[test]
    fun test_swap_xbtc_for_usdt() {
        let scenario = scenario();
        swap_xbtc_for_usdt(&mut scenario);
        end(scenario);
    }

    #[test]
    fun test_withdraw_almost_all() {
        let scenario = scenario();
        withdraw_almost_all(&mut scenario);
        end(scenario);
    }

    #[test]
    fun test_withdraw_all() {
        let scenario = scenario();
        withdraw_all(&mut scenario);
        end(scenario);
    }

    #[test]
    fun test_get_amount_out_does_not_overflow_on_coin_in_close_to_u64_max() {
        let scenario = scenario();
        get_amount_out_does_not_overflow_on_coin_in_close_to_u64_max(&mut scenario);
        end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = swap::implements::ERR_POOL_FULL)]
    fun test_add_liquidity_aborts_if_pool_has_full() {
        let scenario = scenario();
        add_liquidity_aborts_if_pool_has_full(&mut scenario);
        end(scenario);
    }

    #[test]
    fun test_swap_with_value_should_ok() {
        let scenario = scenario();
        swap_with_value_should_ok(&mut scenario);
        end(scenario);
    }

    // Non-sequential tests
    #[test]
    fun test_math() {
        let scenario = scenario();
        test_math_(&mut scenario);
        end(scenario);
    }

    fun add_liquidity_with_register(test: &mut Scenario) {
        let (owner, _) = people();

        next_tx(test, owner);
        {
            implements::init_for_testing(ctx(test));
        };

        next_tx(test, owner);
        {
            let global = test_scenario::take_shared<Global>(test);

            let (lp, _pool_id) = implements::add_liquidity_for_testing<USDT, XBTC>(
                &mut global,
                mint<USDT>(USDT_AMOUNT, ctx(test)),
                mint<XBTC>(XBTC_AMOUNT, ctx(test)),
                ctx(test)
            );

            let burn = burn(lp);
            assert!(burn == sqrt(mul_to_u128(USDT_AMOUNT, XBTC_AMOUNT)) - MINIMAL_LIQUIDITY, burn);

            test_scenario::return_shared(global)
        };

        next_tx(test, owner);
        {
            let global = test_scenario::take_shared<Global>(test);
            let pool = implements::get_mut_pool_for_testing<USDT, XBTC>(&mut global);

            let (reserve_usdt, reserve_xbtc, lp_supply) = implements::get_reserves_size(pool);

            assert!(lp_supply == sqrt(mul_to_u128(USDT_AMOUNT, XBTC_AMOUNT)), lp_supply);
            assert!(reserve_usdt == USDT_AMOUNT, 0);
            assert!(reserve_xbtc == XBTC_AMOUNT, 0);

            test_scenario::return_shared(global)
        };
    }

    /// Expect LP tokens to double in supply when the same values passed
    fun add_liquidity(test: &mut Scenario) {
        add_liquidity_with_register(test);

        let (_, theguy) = people();

        next_tx(test, theguy);
        {
            let global = test_scenario::take_shared<Global>(test);
            let pool = implements::get_mut_pool_for_testing<USDT, XBTC>(&mut global);

            let (reserve_usdt, reserve_xbtc, _lp_supply) = implements::get_reserves_size<USDT, XBTC>(pool);

            let (lp_tokens, _returns) = implements::add_liquidity_for_testing<USDT, XBTC>(
                &mut global,
                mint<USDT>(reserve_usdt / 100, ctx(test)),
                mint<XBTC>(reserve_xbtc / 100, ctx(test)),
                ctx(test)
            );

            let burn = burn(lp_tokens);
            assert!(burn == 137840487, burn);

            test_scenario::return_shared(global)
        };
    }

    fun swap_usdt_for_xbtc(test: &mut Scenario) {
        add_liquidity_with_register(test);

        let (_, the_guy) = people();

        next_tx(test, the_guy);
        {
            let global = test_scenario::take_shared<Global>(test);
            let pool = implements::get_mut_pool_for_testing<USDT, XBTC>(&mut global);
            let (reserve_usdt, reserve_xbtc, _lp_supply) = implements::get_reserves_size<USDT, XBTC>(pool);

            let expected_xbtc = implements::get_amount_out(
                USDT_AMOUNT / 100,
                reserve_usdt,
                reserve_xbtc
            );

            let returns = implements::swap_for_testing<USDT, XBTC>(
                &mut global,
                mint<USDT>(USDT_AMOUNT / 100, ctx(test)),
                1,
                ctx(test)
            );
            assert!(vector::length(&returns) == 4, vector::length(&returns));

            let coin_out = vector::borrow(&returns, 3);
            assert!(*coin_out == expected_xbtc, *coin_out);

            test_scenario::return_shared(global);
        };
    }

    fun swap_xbtc_for_usdt(test: &mut Scenario) {
        swap_usdt_for_xbtc(test);

        let (owner, _) = people();

        next_tx(test, owner);
        {
            let global = test_scenario::take_shared<Global>(test);
            let pool = implements::get_mut_pool_for_testing<USDT, XBTC>(&mut global);
            let (reserve_usdt, reserve_xbtc, _lp_supply) = implements::get_reserves_size<USDT, XBTC>(pool);

            let expected_usdt = implements::get_amount_out(
                XBTC_AMOUNT / 100,
                reserve_xbtc,
                reserve_usdt
            );

            let returns = implements::swap_for_testing<XBTC, USDT>(
                &mut global,
                mint<XBTC>(XBTC_AMOUNT / 100, ctx(test)),
                1,
                ctx(test)
            );
            assert!(vector::length(&returns) == 4, vector::length(&returns));

            let coin_out = vector::borrow(&returns, 1);
            assert!(*coin_out == expected_usdt, expected_usdt);

            test_scenario::return_shared(global);
        };
    }

    fun withdraw_almost_all(test: &mut Scenario) {
        swap_xbtc_for_usdt(test);

        let (owner, _) = people();

        next_tx(test, owner);
        {
            let global = test_scenario::take_shared<Global>(test);
            let pool = implements::get_mut_pool_for_testing<USDT, XBTC>(&mut global);
            let (_reserve_usdt, _reserve_xbtc, to_mint_lp) = implements::get_reserves_size(pool);

            let lp = mint<LP<USDT, XBTC>>(to_mint_lp, ctx(test));

            let (usdt, xbtc) = implements::remove_liquidity_for_testing<USDT, XBTC>(pool, lp, ctx(test));
            let (reserve_usdt, reserve_xbtc, lp_supply) = implements::get_reserves_size(pool);

            assert!(lp_supply == 0, lp_supply);
            assert!(reserve_xbtc == 0, reserve_xbtc);
            assert!(reserve_usdt == 0, reserve_usdt);

            burn(usdt);
            burn(xbtc);

            test_scenario::return_shared(global);
        }
    }

    fun withdraw_all(test: &mut Scenario) {
        swap_xbtc_for_usdt(test);

        let (owner, _) = people();

        next_tx(test, owner);
        {
            let global = test_scenario::take_shared<Global>(test);
            let pool = implements::get_mut_pool_for_testing<USDT, XBTC>(&mut global);
            let (_reserve_usdt, _reserve_xbtc, to_mint_lp) = implements::get_reserves_size(pool);

            let lp = mint<LP<USDT, XBTC>>(to_mint_lp, ctx(test));

            let (usdt, xbtc) = implements::remove_liquidity_for_testing(pool, lp, ctx(test));
            let (reserve_usdt, reserve_xbtc, lp_supply) = implements::get_reserves_size(pool);
            assert!(lp_supply == 0, lp_supply);
            assert!(reserve_usdt == 0, reserve_usdt);
            assert!(reserve_xbtc == 0, reserve_xbtc);


            let (usdt_fee, xbtc_fee, fee_usdt, fee_xbtc) = implements::withdraw_for_testing<USDT, XBTC>(
                &mut global,
                ctx(test)
            );

            // make sure that withdrawn assets
            let burn_usdt = burn(usdt);
            let burn_xbtc = burn(xbtc);
            let burn_usdt_fee = burn(usdt_fee);
            let burn_xbtc_fee = burn(xbtc_fee);

            assert!(burn_usdt_fee == fee_usdt, fee_usdt);
            assert!(burn_xbtc_fee == fee_xbtc, fee_xbtc);
            assert!(burn_usdt == 1899858166476, burn_usdt);
            assert!(burn_xbtc == 100012242, burn_xbtc);

            test_scenario::return_shared(global);
        };
    }

    fun get_amount_out_does_not_overflow_on_coin_in_close_to_u64_max(test: &mut Scenario) {
        let (owner, _) = people();

        next_tx(test, owner);
        {
            implements::init_for_testing(ctx(test));
        };

        let usdt_val = MAX_U64 / 20000;
        let xbtc_val = MAX_U64 / 20000;
        let max_usdt = MAX_U64;

        next_tx(test, owner);
        {
            let global = test_scenario::take_shared<Global>(test);

            let (lp, _pool_id) = implements::add_liquidity_for_testing<USDT, XBTC>(
                &mut global,
                mint<USDT>(usdt_val, ctx(test)),
                mint<XBTC>(xbtc_val, ctx(test)),
                ctx(test)
            );

            let burn = burn(lp);
            assert!(burn == sqrt(mul_to_u128(usdt_val, xbtc_val)) - MINIMAL_LIQUIDITY, burn);

            test_scenario::return_shared(global)
        };

        next_tx(test, owner);
        {
            let global = test_scenario::take_shared<Global>(test);
            let (lp_tokens, _returns) = implements::add_liquidity_for_testing<USDT, XBTC>(
                &mut global,
                mint<USDT>(usdt_val, ctx(test)),
                mint<XBTC>(xbtc_val, ctx(test)),
                ctx(test)
            );

            burn(lp_tokens);

            let pool = implements::get_mut_pool_for_testing<USDT, XBTC>(&mut global);
            let (reserve_usdt, reserve_xbtc, _lp_supply) = implements::get_reserves_size<USDT, XBTC>(pool);

            let _expected_xbtc = implements::get_amount_out(
                max_usdt,
                reserve_usdt,
                reserve_xbtc
            );

            test_scenario::return_shared(global)
        };
    }

    fun add_liquidity_aborts_if_pool_has_full(test: &mut Scenario) {
        let (owner, _) = people();

        next_tx(test, owner);
        {
            implements::init_for_testing(ctx(test));
        };

        let usdt_val = MAX_U64 / 10000;
        let xbtc_val = MAX_U64 / 10000;

        next_tx(test, owner);
        {
            let global = test_scenario::take_shared<Global>(test);

            let (lp, _pool_id) = implements::add_liquidity_for_testing<USDT, XBTC>(
                &mut global,
                mint<USDT>(usdt_val, ctx(test)),
                mint<XBTC>(xbtc_val, ctx(test)),
                ctx(test)
            );
            burn(lp);
            test_scenario::return_shared(global)
        }
    }

    fun swap_with_value_should_ok(test: &mut Scenario) {
        let (owner, _) = people();

        next_tx(test, owner);
        {
            implements::init_for_testing(ctx(test));
        };

        let usdt_val = 184456367;
        let xbtc_val = 70100;

        next_tx(test, owner);
        {
            let global = test_scenario::take_shared<Global>(test);

            let (lp, _pool_id) = implements::add_liquidity_for_testing<USDT, XBTC>(
                &mut global,
                mint<USDT>(usdt_val, ctx(test)),
                mint<XBTC>(xbtc_val, ctx(test)),
                ctx(test)
            );

            let burn = burn(lp);
            assert!(burn == sqrt(mul_to_u128(usdt_val, xbtc_val)) - MINIMAL_LIQUIDITY, burn);

            test_scenario::return_shared(global)
        };

        next_tx(test, owner);
        {
            let global = test_scenario::take_shared<Global>(test);

            let pool = implements::get_mut_pool_for_testing<USDT, XBTC>(&mut global);
            let (reserve_usdt, reserve_xbtc, _lp_supply) = implements::get_reserves_size<USDT, XBTC>(pool);

            assert!(184456367 == reserve_usdt, reserve_usdt);
            assert!(70100 == reserve_xbtc, reserve_xbtc);

            let expected_btc = implements::get_amount_out(
                usdt_val,
                reserve_usdt,
                reserve_xbtc
            );
            assert!(34997 == expected_btc, expected_btc);

            let returns = implements::swap_for_testing<USDT, XBTC>(
                &mut global,
                mint<USDT>(usdt_val, ctx(test)),
                1,
                ctx(test)
            );
            assert!(vector::length(&returns) == 4, vector::length(&returns));
            let coin_out = vector::borrow(&returns, 3);
            assert!(*coin_out == expected_btc, *coin_out);

            test_scenario::return_shared(global)
        };

        next_tx(test, owner);
        {
            let global = test_scenario::take_shared<Global>(test);

            let pool = implements::get_mut_pool_for_testing<USDT, XBTC>(&mut global);
            let (reserve_usdt, reserve_xbtc, _lp_supply) = implements::get_reserves_size<USDT, XBTC>(pool);

            assert!(368802061 == reserve_usdt, reserve_usdt);
            assert!(35103 == reserve_xbtc, reserve_xbtc);

            let expected_usdt = implements::get_amount_out(
                xbtc_val,
                reserve_xbtc,
                reserve_usdt
            );
            assert!(245497690 == expected_usdt, expected_usdt);

            let returns = implements::swap_for_testing<XBTC, USDT>(
                &mut global,
                mint<XBTC>(xbtc_val, ctx(test)),
                1,
                ctx(test)
            );
            assert!(vector::length(&returns) == 4, vector::length(&returns));
            let coin_out = vector::borrow(&returns, 1);
            assert!(*coin_out == expected_usdt, *coin_out);

            test_scenario::return_shared(global)
        }
    }

    /// This just tests the math.
    fun test_math_(_: &mut Scenario) {
        let u64_max = 18446744073709551615;
        let max_val = u64_max / 10000 - 10000;

        // Try small values
        assert!(implements::get_amount_out(10, 1000, 1000) == 9, implements::get_amount_out(10, 1000, 1000));

        // Even with 0 comission there's this small loss of 1
        assert!(
            implements::get_amount_out(10000, max_val, max_val) == 9969,
            implements::get_amount_out(10000, max_val, max_val)
        );
        assert!(
            implements::get_amount_out(1000, max_val, max_val) == 996,
            implements::get_amount_out(1000, max_val, max_val)
        );
        assert!(
            implements::get_amount_out(100, max_val, max_val) == 99,
            implements::get_amount_out(100, max_val, max_val)
        );
    }

    // utilities
    fun scenario(): Scenario { test_scenario::begin(@0x1) }

    fun people(): (address, address) { (@0xBEEF, @0x1337) }
}
