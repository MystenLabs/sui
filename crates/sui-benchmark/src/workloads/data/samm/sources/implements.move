// Copyright 2022 OmniBTC Authors. Licensed under Apache-2.0 License.
module swap::implements {
    use std::ascii::into_bytes;
    use std::string::{Self, String};
    use std::type_name::{get, into_string};
    use std::vector;

    use sui::bag::{Self, Bag};
    use sui::balance::{Self, Supply, Balance};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    use swap::comparator;
    use swap::math;

    friend swap::beneficiary;
    friend swap::controller;
    friend swap::interface;

    /// For when Coin is zero.
    const ERR_ZERO_AMOUNT: u64 = 0;
    /// For when someone tries to swap in an empty pool.
    const ERR_RESERVES_EMPTY: u64 = 1;
    /// For when someone attempts to add more liquidity than u128 Math allows.
    const ERR_POOL_FULL: u64 = 2;
    /// Insuficient amount in coin x reserves.
    const ERR_INSUFFICIENT_COIN_X: u64 = 3;
    /// Insuficient amount in coin y reserves.
    const ERR_INSUFFICIENT_COIN_Y: u64 = 4;
    /// Divide by zero while calling mul_div.
    const ERR_DIVIDE_BY_ZERO: u64 = 5;
    /// For when someone add liquidity with invalid parameters.
    const ERR_OVERLIMIT: u64 = 6;
    /// Amount out less than minimum.
    const ERR_COIN_OUT_NUM_LESS_THAN_EXPECTED_MINIMUM: u64 = 7;
    /// Liquid not enough.
    const ERR_LIQUID_NOT_ENOUGH: u64 = 8;
    /// Coin X is the same as Coin Y
    const ERR_THE_SAME_COIN: u64 = 9;
    /// Pool X-Y has registered
    const ERR_POOL_HAS_REGISTERED: u64 = 10;
    /// Pool X-Y not register
    const ERR_POOL_NOT_REGISTER: u64 = 11;
    /// Coin X and Coin Y order
    const ERR_MUST_BE_ORDER: u64 = 12;
    /// Overflow for u64
    const ERR_U64_OVERFLOW: u64 = 13;
    /// Incorrect swap
    const ERR_INCORRECT_SWAP: u64 = 14;
    /// Insufficient liquidity
    const ERR_INSUFFICIENT_LIQUIDITY_MINTED: u64 = 15;

    /// Current fee is 0.3%
    const FEE_MULTIPLIER: u64 = 30;
    /// The integer scaling setting for fees calculation.
    const FEE_SCALE: u64 = 10000;
    /// The max value that can be held in one of the Balances of
    /// a Pool. U64 MAX / FEE_SCALE
    const MAX_POOL_VALUE: u64 = {
        18446744073709551615 / 10000
    };
    /// Minimal liquidity.
    const MINIMAL_LIQUIDITY: u64 = 1000;
    /// Max u64 value.
    const U64_MAX: u64 = 18446744073709551615;

    const MAX_FEE_RATIO: u64 = 100;
    const MIN_FEE_RATIO: u64 = 10;
    const RATIO_GAP:u64 = 90;
    const RATIO_MULTIPLIER: u64 = 1000;

    /// The Pool token that will be used to mark the pool share
    /// of a liquidity provider. The parameter `X` and `Y` is for the
    /// coin held in the pool.
    struct LP<phantom X, phantom Y> has drop, store {}

    /// The pool with exchange.
    struct Pool<phantom X, phantom Y> has store {
        global: ID,
        coin_x: Balance<X>,
        fee_coin_x: Balance<X>,
        coin_y: Balance<Y>,
        fee_coin_y: Balance<Y>,
        lp_supply: Supply<LP<X, Y>>,
        min_liquidity: Balance<LP<X, Y>>,
    }

    /// The global config
    struct Global has key {
        id: UID,
        has_paused: bool,
        controller: address,
        beneficiary: address,
        pools: Bag,
    }

    /// Init global config
    fun init(ctx: &mut TxContext) {
        let global = Global {
            id: object::new(ctx),
            has_paused: false,
            controller: @controller,
            beneficiary: @beneficiary,
            pools: bag::new(ctx)
        };

        transfer::share_object(global)
    }

    public fun global_id<X, Y>(pool: &Pool<X, Y>): ID {
        pool.global
    }

    public(friend) fun id<X, Y>(global: &Global): ID {
        object::uid_to_inner(&global.id)
    }

    public(friend) fun get_mut_pool<X, Y>(
        global: &mut Global,
        is_order: bool,
    ): &mut Pool<X, Y> {
        assert!(is_order, ERR_MUST_BE_ORDER);

        let lp_name = generate_lp_name<X, Y>();
        let has_registered = bag::contains_with_type<String, Pool<X, Y>>(&global.pools, lp_name);
        assert!(has_registered, ERR_POOL_NOT_REGISTER);

        bag::borrow_mut<String, Pool<X, Y>>(&mut global.pools, lp_name)
    }

    public(friend) fun has_registered<X, Y>(
        global: &Global
    ): bool {
        let lp_name = generate_lp_name<X, Y>();
        bag::contains_with_type<String, Pool<X, Y>>(&global.pools, lp_name)
    }

    public(friend) fun pause(global: &mut Global) {
        global.has_paused = true
    }

    public(friend) fun resume(global: &mut Global) {
        global.has_paused = false
    }

    public(friend) fun modify_controller(global: &mut Global, new_controller: address) {
        global.controller = new_controller
    }

    public(friend) fun is_emergency(global: &Global): bool {
        global.has_paused
    }

    public(friend) fun controller(global: &Global): address {
        global.controller
    }

    public(friend) fun beneficiary(global: &Global): address {
        global.beneficiary
    }

    public fun generate_lp_name<X, Y>(): String {
        let lp_name = string::utf8(b"");
        string::append_utf8(&mut lp_name, b"LP-");

        if (is_order<X, Y>()) {
            string::append_utf8(&mut lp_name, into_bytes(into_string(get<X>())));
            string::append_utf8(&mut lp_name, b"-");
            string::append_utf8(&mut lp_name, into_bytes(into_string(get<Y>())));
        } else {
            string::append_utf8(&mut lp_name, into_bytes(into_string(get<Y>())));
            string::append_utf8(&mut lp_name, b"-");
            string::append_utf8(&mut lp_name, into_bytes(into_string(get<X>())));
        };

        lp_name
    }

    public fun is_order<X, Y>(): bool {
        let comp = comparator::compare(&get<X>(), &get<Y>());
        assert!(!comparator::is_equal(&comp), ERR_THE_SAME_COIN);

        if (comparator::is_smaller_than(&comp)) {
            true
        } else {
            false
        }
    }

    /// Register pool
    public(friend) fun register_pool<X, Y>(
        global: &mut Global,
        is_order: bool
    ) {
        assert!(is_order, ERR_MUST_BE_ORDER);

        let lp_name = generate_lp_name<X, Y>();
        let has_registered = bag::contains_with_type<String, Pool<X, Y>>(&global.pools, lp_name);
        assert!(!has_registered, ERR_POOL_HAS_REGISTERED);

        let lp_supply = balance::create_supply(LP<X, Y> {});
        let new_pool = Pool {
            global: object::uid_to_inner(&global.id),
            coin_x: balance::zero<X>(),
            fee_coin_x: balance::zero<X>(),
            coin_y: balance::zero<Y>(),
            fee_coin_y: balance::zero<Y>(),
            lp_supply,
            min_liquidity: balance::zero<LP<X, Y>>()
        };
        bag::add(&mut global.pools, lp_name, new_pool);
    }

    /// Add liquidity to the `Pool`. Sender needs to provide both
    /// `Coin<X>` and `Coin<Y>`, and in exchange he gets `Coin<LP>` -
    /// liquidity provider tokens.
    public(friend) fun add_liquidity<X, Y>(
        pool: &mut Pool<X, Y>,
        coin_x: Coin<X>,
        coin_x_min: u64,
        coin_y: Coin<Y>,
        coin_y_min: u64,
        is_order: bool,
        ctx: &mut TxContext
    ): (Coin<LP<X, Y>>, vector<u64>) {
        assert!(is_order, ERR_MUST_BE_ORDER);

        let coin_x_value = coin::value(&coin_x);
        let coin_y_value = coin::value(&coin_y);

        assert!(coin_x_value > 0 && coin_y_value > 0, ERR_ZERO_AMOUNT);

        let coin_x_balance = coin::into_balance(coin_x);
        let coin_y_balance = coin::into_balance(coin_y);

        let (coin_x_reserve, coin_y_reserve, lp_supply) = get_reserves_size(pool);
        let (optimal_coin_x, optimal_coin_y) = calc_optimal_coin_values(
            coin_x_value,
            coin_y_value,
            coin_x_min,
            coin_y_min,
            coin_x_reserve,
            coin_y_reserve
        );

        let provided_liq = if (0 == lp_supply) {
            let initial_liq = math::sqrt(math::mul_to_u128(optimal_coin_x, optimal_coin_y));
            assert!(initial_liq > MINIMAL_LIQUIDITY, ERR_LIQUID_NOT_ENOUGH);

            let minimal_liquidity = balance::increase_supply(
                &mut pool.lp_supply,
                MINIMAL_LIQUIDITY
            );
            balance::join(&mut pool.min_liquidity, minimal_liquidity);

            initial_liq - MINIMAL_LIQUIDITY
        } else {
            let x_liq = (lp_supply as u128) * (optimal_coin_x as u128) / (coin_x_reserve as u128);
            let y_liq = (lp_supply as u128) * (optimal_coin_y as u128) / (coin_y_reserve as u128);
            if (x_liq < y_liq) {
                assert!(x_liq < (U64_MAX as u128), ERR_U64_OVERFLOW);
                (x_liq as u64)
            } else {
                assert!(y_liq < (U64_MAX as u128), ERR_U64_OVERFLOW);
                (y_liq as u64)
            }
        };
        assert!(provided_liq > 0, ERR_INSUFFICIENT_LIQUIDITY_MINTED);

        if (optimal_coin_x < coin_x_value) {
            transfer::public_transfer(
                coin::from_balance(balance::split(&mut coin_x_balance, coin_x_value - optimal_coin_x), ctx),
                tx_context::sender(ctx)
            )
        };
        if (optimal_coin_y < coin_y_value) {
            transfer::public_transfer(
                coin::from_balance(balance::split(&mut coin_y_balance, coin_y_value - optimal_coin_y), ctx),
                tx_context::sender(ctx)
            )
        };

        let coin_x_amount = balance::join(&mut pool.coin_x, coin_x_balance);
        let coin_y_amount = balance::join(&mut pool.coin_y, coin_y_balance);

        assert!(coin_x_amount < MAX_POOL_VALUE, ERR_POOL_FULL);
        assert!(coin_y_amount < MAX_POOL_VALUE, ERR_POOL_FULL);

        let balance = balance::increase_supply(&mut pool.lp_supply, provided_liq);

        let return_values = vector::empty<u64>();
        vector::push_back(&mut return_values, coin_x_value);
        vector::push_back(&mut return_values, coin_y_value);
        vector::push_back(&mut return_values, provided_liq);

        (coin::from_balance(balance, ctx), return_values)
    }

    /// Remove liquidity from the `Pool` by burning `Coin<LP>`.
    /// Returns `Coin<X>` and `Coin<Y>`.
    public(friend) fun remove_liquidity<X, Y>(
        pool: &mut Pool<X, Y>,
        lp_coin: Coin<LP<X, Y>>,
        is_order: bool,
        ctx: &mut TxContext
    ): (Coin<X>, Coin<Y>) {
        assert!(is_order, ERR_MUST_BE_ORDER);

        let lp_val = coin::value(&lp_coin);
        assert!(lp_val > 0, ERR_ZERO_AMOUNT);

        let (coin_x_amount, coin_y_amount, lp_supply) = get_reserves_size(pool);
        let coin_x_out = math::mul_div(coin_x_amount, lp_val, lp_supply);
        let coin_y_out = math::mul_div(coin_y_amount, lp_val, lp_supply);

        balance::decrease_supply(&mut pool.lp_supply, coin::into_balance(lp_coin));

        (
            coin::take(&mut pool.coin_x, coin_x_out, ctx),
            coin::take(&mut pool.coin_y, coin_y_out, ctx)
        )
    }

    

    /// Swap Coin<X> for Coin<Y>
    /// Returns Coin<Y>
    public(friend) fun swap_out<X, Y>(
        global: &mut Global,
        coin_in: Coin<X>,
        coin_out_val: u64,
        is_order: bool,
        ctx: &mut TxContext
    ): vector<u64> {
        assert!(coin::value<X>(&coin_in) > 0, ERR_ZERO_AMOUNT);

        if (is_order) {
            let pool = get_mut_pool<X, Y>(global, is_order);
            let (coin_x_reserve, coin_y_reserve, _lp) = get_reserves_size(pool);
            assert!(coin_x_reserve > 0 && coin_y_reserve > 0, ERR_RESERVES_EMPTY);
            let coin_x_in_max = coin::value(&coin_in);
            // give 20 percent of trading fee (0.3% of input) to the fundation
            // This function is just the calculation, I guess 0.3% is already considered in the get_amount_out
            // Need to modify 
            let coin_y_out = coin_out_val;
            // let coin_x_fee = get_fee_to_fundation(coin_x_in);
            let fee_multiplier = get_multiplier_out(coin_y_out, coin_x_reserve, coin_y_reserve);
            let coin_x_in = get_amount_in_samm(
                coin_y_out,
                coin_x_reserve,
                coin_y_reserve,
                fee_multiplier,
            );
            assert!(
                coin_x_in_max >= coin_x_in,
                ERR_COIN_OUT_NUM_LESS_THAN_EXPECTED_MINIMUM
            );

            let coin_x_in_coin = coin::split(&mut coin_in, coin_x_in, ctx);
            transfer::public_transfer(coin_in, tx_context::sender(ctx));
            let coin_x_balance = coin::into_balance(coin_x_in_coin);
            balance::join(&mut pool.coin_x, coin_x_balance);
            let coin_out = coin::take(&mut pool.coin_y, coin_y_out, ctx);
            transfer::public_transfer(coin_out, tx_context::sender(ctx));

            // The division operation truncates the decimal,
            // Causing coin_out_value to be less than the calculated value.
            // Thus making the actual value of new_reserve_out be more.
            // So lp_value is increased.
            let (new_reserve_x, new_reserve_y, _lp) = get_reserves_size(pool);
            assert_lp_value_is_increased(
                coin_x_reserve,
                coin_y_reserve,
                new_reserve_x,
                new_reserve_y
            );

            let return_values = vector::empty<u64>();
            vector::push_back(&mut return_values, coin_x_in);
            vector::push_back(&mut return_values, 0);
            vector::push_back(&mut return_values, 0);
            vector::push_back(&mut return_values, coin_y_out);
            return_values
        } else {
            let pool = get_mut_pool<Y, X>(global, !is_order);
            let (coin_x_reserve, coin_y_reserve, _lp) = get_reserves_size(pool);
            assert!(coin_x_reserve > 0 && coin_y_reserve > 0, ERR_RESERVES_EMPTY);
            let coin_y_in_max = coin::value(&coin_in);
            let coin_x_out = coin_out_val;
            // let coin_y_fee = get_fee_to_fundation(coin_y_in);
            let fee_multiplier = get_multiplier_out(coin_x_out, coin_y_reserve, coin_x_reserve);

            let coin_y_in = get_amount_in_samm(
                coin_x_out,
                coin_y_reserve,
                coin_x_reserve,
                fee_multiplier,
            );
            assert!(
                coin_y_in_max >= coin_y_in,
                ERR_COIN_OUT_NUM_LESS_THAN_EXPECTED_MINIMUM
            );

            let coin_y_in_coin = coin::split(&mut coin_in, coin_y_in, ctx);
            transfer::public_transfer(coin_in, tx_context::sender(ctx));
            let coin_y_in_balance = coin::into_balance(coin_y_in_coin);
            // balance::join(&mut pool.fee_coin_y, balance::split(&mut coin_y_balance, coin_y_fee));
            balance::join(&mut pool.coin_y, coin_y_in_balance);
            // balance::join(&mut pool.fee_coin_y, coin_y_balance);
            let coin_out = coin::take(&mut pool.coin_x, coin_x_out, ctx);
            transfer::public_transfer(coin_out, tx_context::sender(ctx));

            // The division operation truncates the decimal,
            // Causing coin_out_value to be less than the calculated value.
            // Thus making the actual value of new_reserve_out be more.
            // So lp_value is increased.
            let (new_reserve_x, new_reserve_y, _lp) = get_reserves_size(pool);
            assert_lp_value_is_increased(
                coin_x_reserve,
                coin_y_reserve,
                new_reserve_x,
                new_reserve_y
            );

            let return_values = vector::empty<u64>();
            vector::push_back(&mut return_values, 0);
            vector::push_back(&mut return_values, coin_x_out);
            vector::push_back(&mut return_values, coin_y_in);
            vector::push_back(&mut return_values, 0);
            return_values
        }
    }

    /// Calculate amounts needed for adding new liquidity for both `X` and `Y`.
    /// Returns both `X` and `Y` coins amounts.
    public fun calc_optimal_coin_values(
        coin_x_desired: u64,
        coin_y_desired: u64,
        coin_x_min: u64,
        coin_y_min: u64,
        coin_x_reserve: u64,
        coin_y_reserve: u64
    ): (u64, u64) {
        if (coin_x_reserve == 0 && coin_y_reserve == 0) {
            return (coin_x_desired, coin_y_desired)
        } else {
            let coin_y_returned = math::mul_div(coin_x_desired, coin_y_reserve, coin_x_reserve);
            if (coin_y_returned <= coin_y_desired) {
                assert!(coin_y_returned >= coin_y_min, ERR_INSUFFICIENT_COIN_Y);
                return (coin_x_desired, coin_y_returned)
            } else {
                let coin_x_returned = math::mul_div(coin_y_desired, coin_x_reserve, coin_y_reserve);
                assert!(coin_x_returned <= coin_x_desired, ERR_OVERLIMIT);
                assert!(coin_x_returned >= coin_x_min, ERR_INSUFFICIENT_COIN_X);
                return (coin_x_returned, coin_y_desired)
            }
        }
    }

    /// Get most used values in a handy way:
    /// - amount of Coin<X>
    /// - amount of Coin<Y>
    /// - total supply of LP<X,Y>
    public fun get_reserves_size<X, Y>(pool: &Pool<X, Y>): (u64, u64, u64) {
        (
            balance::value(&pool.coin_x),
            balance::value(&pool.coin_y),
            balance::supply_value(&pool.lp_supply)
        )
    }

    public fun get_fee_amount<X, Y>(pool: &Pool<X, Y>): (u64, u64) {
        (
            balance::value(&pool.fee_coin_x),
            balance::value(&pool.fee_coin_y),
        )
    }

    /// return coin_in * 0.3% * 20%
    public fun get_fee_to_fundation(
        coin_in: u64,
    ): u64 {
        // 20% fee to swap fundation
        let fee_multiplier = FEE_MULTIPLIER / 5;

        math::mul_div(coin_in, fee_multiplier, FEE_SCALE)
    }


    public fun get_multiplier(
        coin_in: u64,
        coin_in_reserve: u64,
        coin_out_Reserve: u64,
    ): u64 {
        // 20% fee to swap fundation
        let fac1 = RATIO_MULTIPLIER / math::sqrt_64(coin_in_reserve / coin_in);
        if (fac1 > MAX_FEE_RATIO)
        {
            return MIN_FEE_RATIO
        }
        else
        {
            return MAX_FEE_RATIO - fac1
        }
    }

    public fun get_multiplier_out(
        coin_out: u64,
        coin_in_reserve: u64,
        coin_out_Reserve: u64,
    ): u64 {
        // 20% fee to swap fundation
        let dr = math::mul_div_u128((RATIO_MULTIPLIER as u128), 3, 2);
        let fac1 = math::mul_div_u128((dr as u128),
        (coin_out as u128),
        (coin_out_Reserve as u128)
        );
        if (fac1 > MAX_FEE_RATIO)
        {
            return math::mul_div_u128((MIN_FEE_RATIO as u128), 
            (coin_in_reserve as u128),
            (coin_out_Reserve as u128)  
            )
        }
        else
        {   
            let ratio = MAX_FEE_RATIO - fac1;
            return math::mul_div_u128((ratio as u128), 
                (coin_in_reserve as u128),
                (coin_out_Reserve as u128) 
            )
        }
    }


    public fun get_amount_in_samm(
        coin_out: u64,
        reserve_in: u64,
        reserve_out: u64,
        fee_multiplier: u64,
    ): u64 {
        let tf = math::mul_div(coin_out, fee_multiplier, FEE_SCALE);
        // let coin_out_scale = (coin_out as u128) * (FEE_SCALE as u128);
        // reserve_in size after adding coin_in (scaled to 1000)
        let new_reserve_out = reserve_out - coin_out;
        // Multiply coin_in by the current exchange rate:
        // current_exchange_rate = reserve_out / reserve_in
        // amount_in_after_fees * current_exchange_rate -> amount_out
        let ef = math::mul_div_u128((coin_out as u128), 
            (reserve_in as u128),
            (new_reserve_out as u128)  
        );
        ef + tf
    }

    public fun get_amount_out_samm(
        coin_in: u64,
        reserve_in: u64,
        reserve_out: u64,
        fee_multiplier_raw: u64,
    ): u64 {
        let fee_multiplier = FEE_SCALE - fee_multiplier_raw;
        let coin_in_val_after_fees = (coin_in as u128) * (fee_multiplier as u128);

        // reserve_in size after adding coin_in (scaled to 1000)
        let new_reserve_in = ((reserve_in as u128) * (FEE_SCALE as u128))
            + coin_in_val_after_fees;

        // Multiply coin_in by the current exchange rate:
        // current_exchange_rate = reserve_out / reserve_in
        // amount_in_after_fees * current_exchange_rate -> amount_out
        math::mul_div_u128(coin_in_val_after_fees, // scaled to 1000
            (reserve_out as u128),
            new_reserve_in  // scaled to 1000
        )
    }

    /// Calculate the output amount minus the fee - 0.3%
    public fun get_amount_out(
        coin_in: u64,
        reserve_in: u64,
        reserve_out: u64,
    ): u64 {
        let fee_multiplier = FEE_SCALE - FEE_MULTIPLIER;

        let coin_in_val_after_fees = (coin_in as u128) * (fee_multiplier as u128);

        // reserve_in size after adding coin_in (scaled to 1000)
        let new_reserve_in = ((reserve_in as u128) * (FEE_SCALE as u128))
            + coin_in_val_after_fees;

        // Multiply coin_in by the current exchange rate:
        // current_exchange_rate = reserve_out / reserve_in
        // amount_in_after_fees * current_exchange_rate -> amount_out
        math::mul_div_u128(coin_in_val_after_fees, // scaled to 1000
            (reserve_out as u128),
            new_reserve_in  // scaled to 1000
        )
    }

    public fun assert_lp_value_is_increased(
        old_reserve_x: u64,
        old_reserve_y: u64,
        new_reserve_x: u64,
        new_reserve_y: u64,
    ) {
        // never overflow
        assert!(
            (old_reserve_x as u128) * (old_reserve_y as u128)
                < (new_reserve_x as u128) * (new_reserve_y as u128),
            ERR_INCORRECT_SWAP
        )
    }

    /// Withdraw the fee coins
    public(friend) fun withdraw<X, Y>(
        pool: &mut Pool<X, Y>,
        ctx: &mut TxContext
    ): (Coin<X>, Coin<Y>, u64, u64) {
        let coin_x_fee = balance::value(&pool.fee_coin_x);
        let coin_y_fee = balance::value(&pool.fee_coin_y);

        assert!(coin_x_fee > 0 && coin_y_fee > 0, ERR_ZERO_AMOUNT);

        let fee_coin_x = coin::from_balance(
            balance::split(&mut pool.fee_coin_x, coin_x_fee),
            ctx
        );
        let fee_coin_y = coin::from_balance(
            balance::split(&mut pool.fee_coin_y, coin_y_fee),
            ctx
        );

        (fee_coin_x, fee_coin_y, coin_x_fee, coin_y_fee)
    }

    #[test_only]
    public fun init_for_testing(
        ctx: &mut TxContext
    ) {
        init(ctx)
    }

    #[test_only]
    public fun add_liquidity_for_testing<X, Y>(
        global: &mut Global,
        coin_x: Coin<X>,
        coin_y: Coin<Y>,
        ctx: &mut TxContext
    ): (Coin<LP<X, Y>>, vector<u64>) {
        let is_order = is_order<X, Y>();
        if (!has_registered<X, Y>(global)) {
            register_pool<X, Y>(global, is_order)
        };
        let pool = get_mut_pool<X, Y>(global, is_order);

        add_liquidity(
            pool,
            coin_x,
            1,
            coin_y,
            1,
            is_order,
            ctx
        )
    }

    #[test_only]
    public fun get_mut_pool_for_testing<X, Y>(
        global: &mut Global
    ): &mut Pool<X, Y> {
        get_mut_pool<X, Y>(global, is_order<X, Y>())
    }

    #[test_only]
    public fun swap_for_testing<X, Y>(
        global: &mut Global,
        coin_in: Coin<X>,
        coin_out_min: u64,
        ctx: &mut TxContext
    ): vector<u64> {
        swap_out<X, Y>(
            global,
            coin_in,
            coin_out_min,
            is_order<X, Y>(),
            ctx
        )
    }

    #[test_only]
    public fun remove_liquidity_for_testing<X, Y>(
        pool: &mut Pool<X, Y>,
        lp_coin: Coin<LP<X, Y>>,
        ctx: &mut TxContext
    ): (Coin<X>, Coin<Y>) {
        remove_liquidity<X, Y>(
            pool,
            lp_coin,
            is_order<X, Y>(),
            ctx
        )
    }

    #[test_only]
    public fun withdraw_for_testing<X, Y>(
        global: &mut Global,
        ctx: &mut TxContext
    ): (Coin<X>, Coin<Y>, u64, u64) {
        let pool = get_mut_pool<X, Y>(global, is_order<X, Y>());

        withdraw<X, Y>(
            pool,
            ctx
        )
    }
}
