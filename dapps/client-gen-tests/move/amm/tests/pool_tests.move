#[test_only]
module amm::pool_tests {
    use sui::test_scenario::{Self, Scenario};
    use sui::tx_context::{Self, TxContext};
    use sui::balance::{Self, Balance};
    use sui::transfer;
    use sui::coin::{Self, Coin};
    use amm::pool::{Self, Pool, PoolRegistry, AdminCap, LP};

    const ADMIN: address = @0xABBA;
    const USER: address = @0xB0B;

    // OTWs for currencies used in tests
    struct A has drop {}
    struct B has drop {}
    struct C has drop {}

    fun mint_coin<T>(
        amount: u64, ctx: &mut TxContext
    ): Coin<T> {
        coin::from_balance(
            balance::create_for_testing<T>(amount),
            ctx
        )
    }

    fun scenario_init(sender: address): Scenario {
        let scenario = test_scenario::begin(ADMIN);
        {
            let ctx = test_scenario::ctx(&mut scenario);
            pool::init_for_testing(ctx);
        };
        test_scenario::next_tx(&mut scenario, sender);

        scenario
    }

    fun scenario_create_pool(
        scenario: &mut test_scenario::Scenario,
        init_a: u64,
        init_b: u64,
        lp_fee_bps: u64,
        admin_fee_pct: u64
    ) {
        let registry = test_scenario::take_shared<PoolRegistry>(scenario);
        let ctx = test_scenario::ctx(scenario);

        let init_a = balance::create_for_testing<A>(init_a);
        let init_b = balance::create_for_testing<B>(init_b);

        let lp = pool::create(&mut registry, init_a, init_b, lp_fee_bps, admin_fee_pct, ctx);
        transfer::public_transfer(coin::from_balance(lp, ctx), tx_context::sender(ctx));

        test_scenario::return_shared(registry);
    }

    fun assert_and_destroy_balance<T>(balance: Balance<T>, value: u64) {
        assert!(balance::value(&balance) == value, 0);
        balance::destroy_for_testing(balance);
    }
 
    /* ================= create_pool tests ================= */

    #[test]
    #[expected_failure(abort_code = pool::EZeroInput)]
    fun test_create_pool_fails_on_init_a_zero() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        {
            let registry = test_scenario::take_shared<PoolRegistry>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let init_a = balance::zero<A>();
            let init_b = balance::create_for_testing<B>(100);

            let lp = pool::create(&mut registry, init_a, init_b, 0, 0, ctx);
            transfer::public_transfer(coin::from_balance(lp, ctx), tx_context::sender(ctx));

            test_scenario::return_shared(registry);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = pool::EZeroInput)]
    fun test_create_pool_fails_on_init_b_zero() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        {
            let registry = test_scenario::take_shared<PoolRegistry>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let init_a = balance::create_for_testing<A>(100);
            let init_b = balance::zero<B>();

            let lp = pool::create(&mut registry, init_a, init_b, 0, 0, ctx);
            transfer::public_transfer(coin::from_balance(lp, ctx), tx_context::sender(ctx));

            test_scenario::return_shared(registry);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = pool::EInvalidFeeParam)]
    fun test_create_pool_fails_on_invalid_lp_fee() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        {
            let registry = test_scenario::take_shared<PoolRegistry>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let init_a = balance::create_for_testing<A>(100);
            let init_b = balance::create_for_testing<B>(100);

            let lp = pool::create(&mut registry, init_a, init_b, 10001, 0, ctx);
            transfer::public_transfer(coin::from_balance(lp, ctx), tx_context::sender(ctx));

            test_scenario::return_shared(registry);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = pool::EInvalidFeeParam)]
    fun test_create_pool_fails_on_invalid_admin_fee() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        {
            let registry = test_scenario::take_shared<PoolRegistry>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let init_a = balance::create_for_testing<A>(100);
            let init_b = balance::create_for_testing<B>(100);

            let lp = pool::create(&mut registry, init_a, init_b, 30, 101, ctx); // aborts here
            transfer::public_transfer(coin::from_balance(lp, ctx), tx_context::sender(ctx));

            test_scenario::return_shared(registry);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = pool::EPoolAlreadyExists)]
    fun test_create_pool_fails_on_duplicate_pair() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        {
            let registry = test_scenario::take_shared<PoolRegistry>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let init_a = balance::create_for_testing<A>(200);
            let init_b = balance::create_for_testing<B>(100);

            let lp = pool::create(&mut registry, init_a, init_b, 30, 10, ctx); // aborts here
            transfer::public_transfer(coin::from_balance(lp, ctx), tx_context::sender(ctx));

            test_scenario::return_shared(registry);

        };

        test_scenario::next_tx(scenario, ADMIN);
        {
            let registry = test_scenario::take_shared<PoolRegistry>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let init_a = balance::create_for_testing<A>(200);
            let init_b = balance::create_for_testing<B>(100);

            let lp = pool::create(&mut registry, init_a, init_b, 30, 10, ctx); // aborts here
            transfer::public_transfer(coin::from_balance(lp, ctx), tx_context::sender(ctx));

            test_scenario::return_shared(registry);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = pool::EInvalidPair)]
    fun test_create_pool_fails_on_same_currency_pair() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        {
            let registry = test_scenario::take_shared<PoolRegistry>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let init_a = balance::create_for_testing<A>(200);
            let init_b = balance::create_for_testing<A>(100);

            let lp = pool::create(&mut registry, init_a, init_b, 30, 10, ctx); // aborts here
            transfer::public_transfer(coin::from_balance(lp, ctx), tx_context::sender(ctx));

            test_scenario::return_shared(registry);

        };

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = pool::EInvalidPair)]
    fun test_create_pool_fails_on_currency_pair_wrong_order() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        {
            let registry = test_scenario::take_shared<PoolRegistry>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let init_a = balance::create_for_testing<B>(200);
            let init_b = balance::create_for_testing<A>(100);

            let lp = pool::create(&mut registry, init_a, init_b, 30, 10, ctx); // aborts here
            transfer::public_transfer(coin::from_balance(lp, ctx), tx_context::sender(ctx));

            test_scenario::return_shared(registry);

        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_create_pool() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        {
            let ctx = test_scenario::ctx(scenario);
            pool::init_for_testing(ctx);
        };

        test_scenario::next_tx(scenario, ADMIN);
        {
            let registry = test_scenario::take_shared<PoolRegistry>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let init_a = balance::create_for_testing<A>(200);
            let init_b = balance::create_for_testing<B>(100);

            let lp = pool::create(&mut registry, init_a, init_b, 30, 10, ctx);
            transfer::public_transfer(coin::from_balance(lp, ctx), tx_context::sender(ctx));

            test_scenario::return_shared(registry);
        };

        test_scenario::next_tx(scenario, ADMIN);
        {
            // test pool
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let (a_value, b_value, lp_supply_value) = pool::pool_values(&mut pool);
            assert!(a_value == 200, 0);
            assert!(b_value == 100, 0);
            assert!(lp_supply_value == 141, 0);

            let (lp_fee_bps, admin_fee_pct) = pool::pool_fees(&mut pool);
            assert!(lp_fee_bps == 30, 0);
            assert!(admin_fee_pct == 10, 0);

            test_scenario::return_shared(pool);

            // test admin cap
            let admin_cap = test_scenario::take_from_sender<AdminCap>(scenario);
            test_scenario::return_to_sender(scenario, admin_cap);
        };

        // create another one
        test_scenario::next_tx(scenario, ADMIN);
        {
            let registry = test_scenario::take_shared<PoolRegistry>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let init_a = balance::create_for_testing<A>(200);
            let init_b = balance::create_for_testing<C>(100);

            let lp = pool::create(&mut registry, init_a, init_b, 30, 10, ctx);
            transfer::public_transfer(coin::from_balance(lp, ctx), tx_context::sender(ctx));

            test_scenario::return_shared(registry);
        };

        test_scenario::end(scenario_val);
    }

    /* ================= deposit tests ================= */


    #[test]
    #[expected_failure(abort_code = pool::EZeroInput)]
    fun test_deposit_fails_on_amount_a_zero() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 100, 100, 30, 10);

        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let a = balance::zero<A>();
            let b = balance::create_for_testing<B>(10);
            let (a, b, lp) = pool::deposit(&mut pool, a, b, 1);

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);
            balance::destroy_for_testing(lp);

            test_scenario::return_shared(pool);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = pool::EZeroInput)]
    fun test_deposit_fails_on_amount_b_zero() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 100, 100, 30, 10);

        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let a = balance::create_for_testing<A>(10);
            let b = balance::zero<B>();
            let (a, b, lp) = pool::deposit(&mut pool, a, b, 1);

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);
            balance::destroy_for_testing(lp);

            test_scenario::return_shared(pool);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_deposit_on_empty_pool() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 100, 100, 30, 10);

        // withdraw liquidity to make pool balances 0
        test_scenario::next_tx(scenario, ADMIN);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let lp_coin = test_scenario::take_from_sender<Coin<LP<A,B>>>(scenario);
            let (a, b) = pool::withdraw(&mut pool, coin::into_balance(lp_coin), 0, 0);

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);

            // sanity check
            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 0 && b == 0 && lp == 0, 0);

            test_scenario::return_shared(pool);
        };

        // do the deposit
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let a = balance::create_for_testing<A>(200);
            let b = balance::create_for_testing<B>(100);
            let (a, b, lp) = pool::deposit(&mut pool, a, b, 141);

            // check returned values
            assert!(balance::value(&a) == 0, 0);
            assert!(balance::value(&b) == 0, 0);
            assert!(balance::value(&lp) == 141, 0);

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);
            balance::destroy_for_testing(lp);
            
            // check pool balances
            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 200, 0);
            assert!(b == 100, 0);
            assert!(lp == 141, 0);

            // return
            test_scenario::return_shared(pool);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_deposit() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 100, 50, 30, 10);

        // deposit exact (100, 50, 70); -> (300, 150, 210)
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let a = balance::create_for_testing<A>(200);
            let b = balance::create_for_testing<B>(100);
            let (a, b, lp) = pool::deposit(&mut pool, a, b, 140);

            // check returned values
            assert!(balance::value(&a) == 0, 0);
            assert!(balance::value(&b) == 0, 0);
            assert!(balance::value(&lp) == 140, 0);

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);
            balance::destroy_for_testing(lp);

            // check pool balances
            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 300, 0);
            assert!(b == 150, 0);
            assert!(lp == 210, 0);

            // return
            test_scenario::return_shared(pool);
        };

        // deposit max B (slippage); (300, 150, 210) -> (400, 200, 280)
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let a = balance::create_for_testing<A>(110);
            let b = balance::create_for_testing<B>(50);
            let (a, b, lp) = pool::deposit(&mut pool, a, b, 70);

            // there's extra balance A
            assert!(balance::value(&a) == 10, 0);
            assert!(balance::value(&b) == 0, 0);
            assert!(balance::value(&lp) == 70, 0);

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);
            balance::destroy_for_testing(lp);

            // check pool balances
            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 400, 0);
            assert!(b == 200, 0);
            assert!(lp == 280, 0);

            // return
            test_scenario::return_shared(pool);
        };

        // deposit max A (slippage); (400, 200, 280) -> (500, 250, 350)
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let a = balance::create_for_testing<A>(100);
            let b = balance::create_for_testing<B>(60);
            let (a, b, lp) = pool::deposit(&mut pool, a, b, 70);

            // there's extra balance B
            assert!(balance::value(&a) == 0, 0);
            assert!(balance::value(&b) == 10, 0);
            assert!(balance::value(&lp) == 70, 0);

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);
            balance::destroy_for_testing(lp);

            // pool balances
            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 500, 0);
            assert!(b == 250, 0);
            assert!(lp == 350, 0);

            // return
            test_scenario::return_shared(pool);
        };

        // no lp issued when input small; (500, 250, 350) -> (501, 251, 350)
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let a = balance::create_for_testing<A>(1);
            let b = balance::create_for_testing<B>(1);
            let (a, b, lp) = pool::deposit(&mut pool, a, b, 0);

            // no lp issued and input balances are fully used up
            assert!(balance::value(&a) == 0, 0);
            assert!(balance::value(&b) == 0, 0);
            assert!(balance::value(&lp) == 0, 0);

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);
            balance::destroy_for_testing(lp);

            // check pool balances
            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 501, 0);
            assert!(b == 251, 0);
            assert!(lp == 350, 0);

            // return
            test_scenario::return_shared(pool);
        };

        test_scenario::end(scenario_val);
    } 

    #[test]
    #[expected_failure(abort_code = pool::EExcessiveSlippage)]
    fun test_deposit_fails_on_min_lp_out() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 100, 100, 30, 10);

        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let a = balance::create_for_testing<A>(200);
            let b = balance::create_for_testing<B>(200);
            let (a, b, lp) = pool::deposit(&mut pool, a, b, 201); // aborts here

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);
            balance::destroy_for_testing(lp);

            test_scenario::return_shared(pool);
        };

        test_scenario::end(scenario_val);
    }

    /* ================= withdraw tests ================= */

    #[test]
    #[expected_failure(abort_code = pool::EZeroInput)]
    fun test_withdraw_fails_on_zero_input() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 100, 100, 30, 10);

        test_scenario::next_tx(scenario, ADMIN);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let lp = balance::zero();
            let (a, b) = pool::withdraw(&mut pool, lp, 0, 0); // aborts here

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);

            test_scenario::return_shared(pool);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_withdraw() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 100, 13, 30, 10);

        // withdraw (100, 13, 36) -> (64, 9, 23)
        test_scenario::next_tx(scenario, ADMIN);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);
            let lp_coin = test_scenario::take_from_sender<Coin<LP<A, B>>>(scenario);
            assert!(coin::value(&lp_coin) == 36, 0); // sanity check

            let ctx = test_scenario::ctx(scenario);
            let lp_in = coin::into_balance(coin::split(&mut lp_coin, 13, ctx));
            let (a, b) = pool::withdraw(&mut pool, lp_in, 36, 4);

            // check output balances
            assert!(balance::value(&a) == 36, 0);
            assert!(balance::value(&b) == 4, 0);

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);

            // check pool values
            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 64, 0);
            assert!(b == 9, 0);
            assert!(lp == 23, 0);

            test_scenario::return_shared(pool);
            test_scenario::return_to_sender(scenario, lp_coin);
        };

        // withdraw small amount (64, 9, 23) -> (62, 9, 22)
        test_scenario::next_tx(scenario, ADMIN);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);
            let lp_coin = test_scenario::take_from_sender<Coin<LP<A, B>>>(scenario);
            assert!(coin::value(&lp_coin) == 23, 0); // sanity check

            let ctx = test_scenario::ctx(scenario);

            let lp_in = coin::into_balance(coin::split(&mut lp_coin, 1, ctx));
            let (a, b) = pool::withdraw(&mut pool, lp_in, 2, 0);

            // check output balances
            assert!(balance::value(&a) == 2, 0);
            assert!(balance::value(&b) == 0, 0);

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);

            // check pool values
            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 62, 0);
            assert!(b == 9, 0);
            assert!(lp == 22, 0);

            test_scenario::return_shared(pool);
            test_scenario::return_to_sender(scenario, lp_coin);
        };

        // withdraw all (62, 9, 22) -> (0, 0, 0)
        test_scenario::next_tx(scenario, ADMIN);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);
            let lp_coin = test_scenario::take_from_sender<Coin<LP<A, B>>>(scenario);

            let lp_in = coin::into_balance(lp_coin);
            let (a, b) = pool::withdraw(&mut pool, lp_in, 62, 9);

            // check output balances
            assert!(balance::value(&a) == 62, 0);
            assert!(balance::value(&b) == 9, 0);

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);

            // check pool values
            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 0, 0);
            assert!(b == 0, 0);
            assert!(lp == 0, 0);

            test_scenario::return_shared(pool);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = pool::EExcessiveSlippage)]
    fun test_withdraw_fails_on_min_a_out() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 100, 100, 30, 10);

        test_scenario::next_tx(scenario, ADMIN);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);
            let lp_coin = test_scenario::take_from_sender<Coin<LP<A, B>>>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let lp_in = coin::into_balance(coin::split(&mut lp_coin, 50, ctx));
            let (a, b) = pool::withdraw(&mut pool, lp_in, 51, 50); // aborts here

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);

            test_scenario::return_shared(pool);
            test_scenario::return_to_sender(scenario, lp_coin);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = pool::EExcessiveSlippage)]
    fun test_withdraw_fails_on_min_b_out() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 100, 100, 30, 10);

        test_scenario::next_tx(scenario, ADMIN);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);
            let lp_coin = test_scenario::take_from_sender<Coin<LP<A, B>>>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let lp_in = coin::into_balance(coin::split(&mut lp_coin, 50, ctx));
            let (a, b) = pool::withdraw(&mut pool, lp_in, 50, 51); // aborts here

            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);

            test_scenario::return_shared(pool);
            test_scenario::return_to_sender(scenario, lp_coin);
        };

        test_scenario::end(scenario_val);
    }

    /* ================= swap tests ================= */

    #[test]
    #[expected_failure(abort_code = pool::EZeroInput)]
    fun test_swap_a_fails_on_zero_input_a() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 100, 100, 30, 10);

        test_scenario::next_tx(scenario, ADMIN);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let a_in = balance::zero<A>();
            let b_out = pool::swap_a(&mut pool, a_in, 0);

            balance::destroy_for_testing(b_out);

            test_scenario::return_shared(pool);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = pool::EZeroInput)]
    fun test_swap_b_fails_on_zero_input_b() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 100, 100, 30, 10);

        test_scenario::next_tx(scenario, ADMIN);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let b_in = balance::zero<B>();
            let a_out = pool::swap_b(&mut pool, b_in, 0);

            balance::destroy_for_testing(a_out);

            test_scenario::return_shared(pool);
        }; 

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = pool::ENoLiquidity)]
    fun test_swap_a_fails_on_zero_pool_balances() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 100, 100, 30, 10);

        test_scenario::next_tx(scenario, ADMIN);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);
            let lp_coin = test_scenario::take_from_sender<Coin<LP<A, B>>>(scenario);

            let (a, b) = pool::withdraw(&mut pool, coin::into_balance(lp_coin), 0, 0);
            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);

            let a_in = balance::create_for_testing<A>(10);
            let b = pool::swap_a(&mut pool, a_in, 0); // aborts here

            balance::destroy_for_testing(b);

            test_scenario::return_shared(pool);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = pool::ENoLiquidity)]
    fun test_swap_b_fails_on_zero_pool_balances() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 100, 100, 30, 10);

        test_scenario::next_tx(scenario, ADMIN);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);
            let lp_coin = test_scenario::take_from_sender<Coin<LP<A, B>>>(scenario);

            let (a, b) = pool::withdraw(&mut pool, coin::into_balance(lp_coin), 0, 0);
            balance::destroy_for_testing(a);
            balance::destroy_for_testing(b);

            let b_in = balance::create_for_testing<B>(10); // aborts here
            let a = pool::swap_b(&mut pool, b_in, 0);

            balance::destroy_for_testing(a);

            test_scenario::return_shared(pool);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_swap_a_without_lp_fees() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 200, 100, 0, 10);

        // swap; (200, 100, 141) -> (213, 94, 141)
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let a_in = balance::create_for_testing<A>(13);
            let b_out = pool::swap_a(&mut pool, a_in, 6);

            // check
            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 213, 0);
            assert!(b == 94, 0);
            assert!(lp == 141, 0);
            assert!(balance::value(&b_out) == 6, 0);
            // admin fees should also be 0 because they're calcluated
            // as percentage of lp fees
            assert!(pool::pool_admin_fee_value(&pool) == 0, 0); 

            test_scenario::return_shared(pool);
            balance::destroy_for_testing(b_out);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_swap_b_without_lp_fees() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 200, 100, 0, 10);

        // swap; (200, 100, 141) -> (177, 113, 141)
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let b_in = balance::create_for_testing<B>(13);
            let a_out = pool::swap_b(&mut pool, b_in, 23);

            // check
            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 177, 0);
            assert!(b == 113, 0);
            assert!(lp == 141, 0);
            assert!(balance::value(&a_out) == 23, 0);
            // admin fees should also be 0 because they're calcluated
            // as percentage of lp fees
            assert!(pool::pool_admin_fee_value(&pool) == 0, 0); 

            test_scenario::return_shared(pool);
            balance::destroy_for_testing(a_out);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_swap_a_with_lp_fees() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 20000, 10000, 30, 0); // lp fee 30 bps

        // swap; (20000, 10000, 14142) -> (21300, 9302, 14142)
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let a_in = balance::create_for_testing<A>(1300);
            let b_out = pool::swap_a(&mut pool, a_in, 608);

            // check
            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 21300, 0);
            assert!(b == 9392, 0);
            assert!(lp == 14142, 0);
            assert!(balance::value(&b_out) == 608, 0);
            assert!(pool::pool_admin_fee_value(&pool) == 0, 0); 

            test_scenario::return_shared(pool);
            balance::destroy_for_testing(b_out);
        };

        // swap small amount; (21300, 9302, 14142) -> (21301, 9302, 14142)
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let a_in = balance::create_for_testing<A>(1);
            let b_out = pool::swap_a(&mut pool, a_in, 0);

            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 21301, 0);
            assert!(b == 9392, 0);
            assert!(lp == 14142, 0);
            assert!(balance::value(&b_out) == 0, 0);
            assert!(pool::pool_admin_fee_value(&pool) == 0, 0); 

            test_scenario::return_shared(pool);
            balance::destroy_for_testing(b_out);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_swap_b_with_lp_fees() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 20000, 10000, 30, 0); // lp fee 30 bps

        // swap; (20000, 10000, 14142) -> (17706, 11300, 14142)
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let b_in = balance::create_for_testing<B>(1300);
            let a_out = pool::swap_b(&mut pool, b_in, 2294);

            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 17706, 0);
            assert!(b == 11300, 0);
            assert!(lp == 14142, 0);
            assert!(balance::value(&a_out) == 2294, 0);
            assert!(pool::pool_admin_fee_value(&pool) == 0, 0); 

            test_scenario::return_shared(pool);
            balance::destroy_for_testing(a_out);
        };

        // swap small amount; (17706, 11300, 14142) -> (17706, 11301, 14142)
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let b_in = balance::create_for_testing<B>(1);
            let a_out = pool::swap_b(&mut pool, b_in, 0);

            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 17706, 0);
            assert!(b == 11301, 0);
            assert!(lp == 14142, 0);
            assert!(balance::value(&a_out) == 0, 0);
            assert!(pool::pool_admin_fee_value(&pool) == 0, 0); 

            test_scenario::return_shared(pool);
            balance::destroy_for_testing(a_out);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_swap_a_with_admin_fees() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 20000, 10000, 30, 30);

        // swap; (20000, 10000, 14142) -> (25000, 8005, 14143)
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let a_in = balance::create_for_testing<A>(5000);
            let b_out = pool::swap_a(&mut pool, a_in, 1995);

            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 25000, 0);
            assert!(b == 8005, 0);
            assert!(lp == 14143, 0);
            assert!(balance::value(&b_out) == 1995, 0);
            assert!(pool::pool_admin_fee_value(&pool) == 1, 0); 

            test_scenario::return_shared(pool);
            balance::destroy_for_testing(b_out);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_swap_b_with_admin_fees() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 20000, 10000, 30, 30);

        // swap; (20000, 10000, 14142) -> (13002, 15400, 14144)
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let b_in = balance::create_for_testing<B>(5400);
            let a_out = pool::swap_b(&mut pool, b_in, 6998);

            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 13002, 0);
            assert!(b == 15400, 0);
            assert!(lp == 14144, 0);
            assert!(balance::value(&a_out) == 6998, 0);
            assert!(pool::pool_admin_fee_value(&pool) == 2, 0); 

            test_scenario::return_shared(pool);
            balance::destroy_for_testing(a_out);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    public fun test_admin_fees_are_correct() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 10_000_000, 10_000_000, 30, 100);

        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let out = pool::swap_a(&mut pool, balance::create_for_testing(10_000), 0);
            assert_and_destroy_balance(out, 9960);

            let (a, b, lp) = pool::pool_values(&pool);
            assert!(a == 10_010_000, 0);
            assert!(b == 9_990_040, 0);
            assert!(lp == 10_000_014, 0); 
            assert!(pool::pool_admin_fee_value(&pool) == 14, 0);

            test_scenario::return_shared(pool);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = pool::EExcessiveSlippage)]
    fun test_swap_a_fails_on_min_out() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 200, 100, 0, 10);

        // swap; (200, 100, 141) -> (213, 94, 141)
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let a_in = balance::create_for_testing<A>(13);
            let b_out = pool::swap_a(&mut pool, a_in, 7); // aborts here

            balance::destroy_for_testing(b_out);
            test_scenario::return_shared(pool);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = pool::EExcessiveSlippage)]
    fun test_swap_b_fails_on_min_out() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 200, 100, 0, 10);

        // swap; (200, 100, 141) -> (177, 113, 141)
        test_scenario::next_tx(scenario, USER);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);

            let b_in = balance::create_for_testing<B>(13);
            let a_out = pool::swap_b(&mut pool, b_in, 24); // aborts here

            test_scenario::return_shared(pool);
            balance::destroy_for_testing(a_out);
        };

        test_scenario::end(scenario_val);
    }

    /* ================= admin fee withdraw tests ================= */

    #[test]
    fun test_admin_withdraw_fees() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 20000, 10000, 30, 30);

        // generate fees and withdraw 1 
        test_scenario::next_tx(scenario, ADMIN);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);
            let cap = test_scenario::take_from_sender<AdminCap>(scenario);

            // generate fees
            let b_in = balance::create_for_testing<B>(5400);
            let a_out = pool::swap_b(&mut pool, b_in, 6998);
            balance::destroy_for_testing(a_out);
            assert!(pool::pool_admin_fee_value(&pool) == 2, 0); // sanity check

            // withdraw
            let fees_out = pool::admin_withdraw_fees(&mut pool, &cap, 1);

            assert!(pool::pool_admin_fee_value(&pool) == 1, 0);
            assert!(balance::value(&fees_out) == 1, 0);

            test_scenario::return_shared(pool);
            test_scenario::return_to_sender(scenario, cap);
            balance::destroy_for_testing(fees_out);
        };

        // withdraw all
        test_scenario::next_tx(scenario, ADMIN);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);
            let cap = test_scenario::take_from_sender<AdminCap>(scenario);

            // withdraw
            let fees_out = pool::admin_withdraw_fees(&mut pool, &cap, 0);

            assert!(pool::pool_admin_fee_value(&pool) == 0, 0);
            assert!(balance::value(&fees_out) == 1, 0);

            test_scenario::return_shared(pool);
            test_scenario::return_to_sender(scenario, cap);
            balance::destroy_for_testing(fees_out);
        };

        test_scenario::end(scenario_val);
    }

    #[test]
    fun test_admin_withdraw_fees_amount_0_and_balance_0() {
        let scenario_val = scenario_init(ADMIN);
        let scenario = &mut scenario_val;
        scenario_create_pool(scenario, 20000, 10000, 30, 30);

        // generate fees and withdraw 1 
        test_scenario::next_tx(scenario, ADMIN);
        {
            let pool = test_scenario::take_shared<Pool<A, B>>(scenario);
            let cap = test_scenario::take_from_sender<AdminCap>(scenario);

            // withdraw
            let fees_out = pool::admin_withdraw_fees(&mut pool, &cap, 0);

            // check
            assert!(balance::value(&fees_out) == 0, 0);

            test_scenario::return_shared(pool);
            test_scenario::return_to_sender(scenario, cap);
            balance::destroy_for_testing(fees_out);
        };
        
        test_scenario::end(scenario_val);
    }
}
