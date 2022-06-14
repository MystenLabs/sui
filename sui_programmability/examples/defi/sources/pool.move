// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example implementation of a liquidity Pool for Sui.
///
/// - Only module publisher can create new Pools.
/// - For simplicity's sake all swaps are done with SUI coin.
/// - Fees are customizable per Pool.
/// - Max stored value for both tokens is: U64_MAX / 10_000
///
/// This solution is rather simple and is based on the example from the Move repo:
/// https://github.com/move-language/move/blob/main/language/documentation/examples/experimental/coin-swap/sources/CoinSwap.move
module defi::pool {
    use sui::Coin::{Self, Coin, TreasuryCap};
    use sui::Balance::{Self, Balance};
    use sui::ID::{VersionedID};
    use sui::SUI::SUI;
    use sui::Transfer;
    use sui::TxContext::{Self, TxContext};

    /// For when supplied Coin is zero.
    const EZeroAmount: u64 = 0;

    /// For when pool fee is set incorrectly.
    /// Allowed values are: [0-10000).
    const EWrongFee: u64 = 1;

    /// For when someone tries to swap in an empty pool.
    const EReservesEmpty: u64 = 2;

    /// For when initial LSP amount is zero.
    const EShareEmpty: u64 = 3;

    /// For when someone attemps to add more liquidity than u128 Math allows.
    const EPoolFull: u64 = 4;

    /// The integer scaling setting for fees calculation.
    const FEE_SCALING: u128 = 10000;

    /// The max value that can be held in one of the Balances of
    /// a Pool. U64 MAX / FEE_SCALING
    const MAX_POOL_VALUE: u64 = {
        18446744073709551615 / 10000
    };

    /// The Pool token that will be used to mark the pool share
    /// of a liquidity provider. The first type parameter stands
    /// for the witness type of a pool. The seconds is for the
    /// coin held in the pool.
    struct LSP<phantom P, phantom T> has drop {}

    /// The pool with exchange.
    ///
    /// - `fee_percent` should be in the range: 0-1000, meaning
    /// that 1000 is 100% and 1 is 0.1%
    struct Pool<phantom P, phantom T> has key {
        id: VersionedID,
        sui: Balance<SUI>,
        token: Balance<T>,
        lsp_treasury: TreasuryCap<LSP<P, T>>,
        fee_percent: u64
    }

    /// Module initializer is empty - to publish a new Pool one has
    /// to create a type which will mark LSPs.
    fun init(_: &mut TxContext) {}

    /// Entrypoint for the `create_pool` method.
    entry fun create_pool_<P: drop, T>(
        witness: P,
        token: Coin<T>,
        sui: Coin<SUI>,
        share: u64,
        fee_percent: u64,
        ctx: &mut TxContext
    ) {
        Transfer::transfer(
            create_pool(witness, token, sui, share, fee_percent, ctx),
            TxContext::sender(ctx)
        )
    }

    /// Create new `Pool` for token `T`. Each Pool holds a `Coin<T>`
    /// and a `Coin<SUI>`. Swaps are available in both directions.
    ///
    /// - `share` argument defines the initial amount of LSP tokens
    /// received by the creator of the Pool.
    public fun create_pool<P: drop, T>(
        _: P,
        token: Coin<T>,
        sui: Coin<SUI>,
        share: u64,
        fee_percent: u64,
        ctx: &mut TxContext
    ): Coin<LSP<P, T>> {
        let sui_amt = Coin::value(&sui);
        let tok_amt = Coin::value(&token);

        assert!(sui_amt > 0, EZeroAmount);
        assert!(tok_amt > 0, EZeroAmount);
        assert!(sui_amt < MAX_POOL_VALUE, EPoolFull);
        assert!(tok_amt < MAX_POOL_VALUE, EPoolFull);
        assert!(fee_percent >= 0 && fee_percent < 10000, EWrongFee);
        assert!(share > 0, EShareEmpty);

        let lsp_treasury = Coin::create_currency(LSP<P, T> {}, ctx);
        let lsp = Coin::mint(share, &mut lsp_treasury, ctx);

        Transfer::share_object(Pool {
            id: TxContext::new_id(ctx),
            token: Coin::into_balance(token),
            sui: Coin::into_balance(sui),
            lsp_treasury,
            fee_percent
        });

        lsp
    }


    /// Entrypoint for the `swap_sui` method. Sends swapped token
    /// to sender.
    entry fun swap_sui_<P, T>(
        pool: &mut Pool<P, T>, sui: Coin<SUI>, ctx: &mut TxContext
    ) {
        Transfer::transfer(
            swap_sui(pool, sui, ctx),
            TxContext::sender(ctx)
        )
    }

    /// Swap `Coin<SUI>` for the `Coin<T>`.
    /// Returns Coin<T>.
    public fun swap_sui<P, T>(
        pool: &mut Pool<P, T>, sui: Coin<SUI>, ctx: &mut TxContext
    ): Coin<T> {
        assert!(Coin::value(&sui) > 0, EZeroAmount);

        let sui_balance = Coin::into_balance(sui);

        // Calculate the output amount - fee
        let (sui_reserve, token_reserve, _) = get_amounts(pool);
        let output_amount = get_input_price(
            Balance::value(&sui_balance),
            sui_reserve,
            token_reserve,
            pool.fee_percent
        );

        Balance::join(&mut pool.sui, sui_balance);
        Coin::withdraw(&mut pool.token, output_amount, ctx)
    }

    /// Entry point for the `swap_token` method. Sends swapped SUI
    /// to the sender.
    entry fun swap_token_<P, T>(
        pool: &mut Pool<P, T>, token: Coin<T>, ctx: &mut TxContext
    ) {
        Transfer::transfer(
            swap_token(pool, token, ctx),
            TxContext::sender(ctx)
        )
    }

    /// Swap `Coin<T>` for the `Coin<SUI>`.
    /// Returns the swapped `Coin<SUI>`.
    public fun swap_token<P, T>(
        pool: &mut Pool<P, T>, token: Coin<T>, ctx: &mut TxContext
    ): Coin<SUI> {
        assert!(Coin::value(&token) > 0, EZeroAmount);

        let tok_balance = Coin::into_balance(token);
        let (sui_reserve, token_reserve, _) = get_amounts(pool);
        let output_amount = get_input_price(
            Balance::value(&tok_balance),
            token_reserve,
            sui_reserve,
            pool.fee_percent
        );

        assert!(sui_reserve > 0 && token_reserve > 0, EReservesEmpty);

        Balance::join(&mut pool.token, tok_balance);
        Coin::withdraw(&mut pool.sui, output_amount, ctx)
    }

    /// Entrypoint for the `add_liquidity` method. Sends `Coin<LSP>` to
    /// the transaction sender.
    entry fun add_liquidity_<P, T>(
        pool: &mut Pool<P, T>, sui: Coin<SUI>, token: Coin<T>, ctx: &mut TxContext
    ) {
        Transfer::transfer(
            add_liquidity(pool, sui, token, ctx),
            TxContext::sender(ctx)
        );
    }

    /// Add liquidity to the `Pool`. Sender needs to provide both
    /// `Coin<SUI>` and `Coin<T>`, and in exchange he gets `Coin<LSP>` -
    /// liquidity provider tokens.
    public fun add_liquidity<P, T>(
        pool: &mut Pool<P, T>, sui: Coin<SUI>, token: Coin<T>, ctx: &mut TxContext
    ): Coin<LSP<P, T>> {
        assert!(Coin::value(&sui) > 0, EZeroAmount);
        assert!(Coin::value(&token) > 0, EZeroAmount);

        let sui_balance = Coin::into_balance(sui);
        let token_balance = Coin::into_balance(token);

        let (sui_amount, _, lsp_supply) = get_amounts(pool);

        let sui_added = Balance::value(&sui_balance);
        let share_minted = (sui_added * lsp_supply) / sui_amount;

        let sui_amt = Balance::join(&mut pool.sui, sui_balance);
        let tok_amt = Balance::join(&mut pool.token, token_balance);

        assert!(sui_amt < MAX_POOL_VALUE, EPoolFull);
        assert!(tok_amt < MAX_POOL_VALUE, EPoolFull);

        Coin::mint(share_minted, &mut pool.lsp_treasury, ctx)
    }

    /// Entrypoint for the `remove_liquidity` method. Transfers
    /// withdrawn assets to the sender.
    entry fun remove_liquidity_<P, T>(
        pool: &mut Pool<P, T>,
        lsp: Coin<LSP<P, T>>,
        ctx: &mut TxContext
    ) {
        let (sui, token) = remove_liquidity(pool, lsp, ctx);
        let sender = TxContext::sender(ctx);

        Transfer::transfer(sui, sender);
        Transfer::transfer(token, sender);
    }

    /// Remove liquidity from the `Pool` by burning `Coin<LSP>`.
    /// Returns `Coin<T>` and `Coin<SUI>`.
    public fun remove_liquidity<P, T>(
        pool: &mut Pool<P, T>,
        lsp: Coin<LSP<P, T>>,
        ctx: &mut TxContext
    ): (Coin<SUI>, Coin<T>) {
        let lsp_amount = Coin::value(&lsp);

        // If there's a non-empty LSP, we can
        assert!(lsp_amount > 0, EZeroAmount);

        let (sui_amt, tok_amt, lsp_supply) = get_amounts(pool);
        let sui_removed = (sui_amt * lsp_amount) / lsp_supply;
        let tok_removed = (tok_amt * lsp_amount) / lsp_supply;

        Coin::burn(lsp, &mut pool.lsp_treasury);

        (
            Coin::withdraw(&mut pool.sui, sui_removed, ctx),
            Coin::withdraw(&mut pool.token, tok_removed, ctx)
        )
    }

    /// Public getter for the price of SUI in token T.
    /// - How much SUI one will get if they send `to_sell` amount of T;
    public fun sui_price<P, T>(pool: &Pool<P, T>, to_sell: u64): u64 {
        let (sui_amt, tok_amt, _) = get_amounts(pool);
        get_input_price(to_sell, tok_amt, sui_amt, pool.fee_percent)
    }

    /// Public getter for the price of token T in SUI.
    /// - How much T one will get if they send `to_sell` amount of SUI;
    public fun token_price<P, T>(pool: &Pool<P, T>, to_sell: u64): u64 {
        let (sui_amt, tok_amt, _) = get_amounts(pool);
        get_input_price(to_sell, sui_amt, tok_amt, pool.fee_percent)
    }


    /// Get most used values in a handy way:
    /// - amount of SUI
    /// - amount of token
    /// - total supply of LSP
    public fun get_amounts<P, T>(pool: &Pool<P, T>): (u64, u64, u64) {
        (
            Balance::value(&pool.sui),
            Balance::value(&pool.token),
            Coin::total_supply(&pool.lsp_treasury)
        )
    }

    /// Calculate the output amount minus the fee - 0.3%
    public fun get_input_price(
        input_amount: u64, input_reserve: u64, output_reserve: u64, fee_percent: u64
    ): u64 {
        // up casts
        let (
            input_amount,
            input_reserve,
            output_reserve,
            fee_percent
        ) = (
            (input_amount as u128),
            (input_reserve as u128),
            (output_reserve as u128),
            (fee_percent as u128)
        );

        let input_amount_with_fee = input_amount * (FEE_SCALING - fee_percent); // 0.3% fee
        let numerator = input_amount_with_fee * output_reserve;
        let denominator = (input_reserve * FEE_SCALING) + input_amount_with_fee;

        (numerator / denominator as u64)
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) {
        init(ctx)
    }
}

#[test_only]
/// Tests for the pool module.
/// They are sequential and based on top of each other.
/// ```
/// * - test_init_pool
/// |   +-- test_creation
/// |       +-- test_swap_sui
/// |           +-- test_swap_tok
/// |               +-- test_withdraw_all
/// ```
module defi::pool_tests {
    use sui::SUI::SUI;
    use sui::Coin::{mint_for_testing as mint, destroy_for_testing as burn};
    use sui::TestScenario::{Self as test, Scenario, next_tx, ctx};
    use defi::pool::{Self, Pool, LSP};

    /// Gonna be our test token.
    struct BEEP {}

    /// A witness type for the pool creation;
    /// The pool provider's identifier.
    struct POOLEY has drop {}

    // Tests section
    #[test] fun test_init_pool() { test_init_pool_(&mut scenario()) }
    #[test] fun test_swap_sui() { test_swap_sui_(&mut scenario()) }
    #[test] fun test_swap_tok() { test_swap_tok_(&mut scenario()) }
    #[test] fun test_withdraw_all() { test_withdraw_all_(&mut scenario()) }

    // Non-sequential tests
    #[test] fun test_math() { test_math_(&mut scenario()) }

    /// Init a Pool with a 1_000_000 BEEP and 1_000_000_000 SUI;
    /// Set the ratio BEEP : SUI = 1 : 1000.
    /// Set LSP token amount to 1000;
    fun test_init_pool_(test: &mut Scenario) {
        let (owner, _) = people();

        next_tx(test, &owner); {
            pool::init_for_testing(ctx(test));
        };

        next_tx(test, &owner); {
            let lsp = pool::create_pool(
                POOLEY {},
                mint<BEEP>(1000000, ctx(test)),
                mint<SUI>(1000000000, ctx(test)),
                1000,
                3,
                ctx(test)
            );

            assert!(burn(lsp) == 1000, 0);
        };

        next_tx(test, &owner); {
            let pool = test::take_shared<Pool<POOLEY, BEEP>>(test);
            let pool_mut = test::borrow_mut(&mut pool);
            let (amt_sui, amt_tok, lsp_supply) = pool::get_amounts(pool_mut);

            assert!(lsp_supply == 1000, 0);
            assert!(amt_sui == 1000000000, 0);
            assert!(amt_tok == 1000000, 0);

            test::return_shared(test, pool);
        };
    }

    /// The other guy tries to exchange 5_000_000 sui for ~ 5000 BEEP,
    /// minus the commission that is paid to the pool.
    fun test_swap_sui_(test: &mut Scenario) {
        test_init_pool_(test);

        let (_, the_guy) = people();

        next_tx(test, &the_guy); {
            let pool = test::take_shared<Pool<POOLEY, BEEP>>(test);
            let pool_mut = test::borrow_mut(&mut pool);

            let token = pool::swap_sui(pool_mut, mint<SUI>(5000000, ctx(test)), ctx(test));

            // Check the value of the coin received by the guy.
            // Due to rounding problem the value is not precise
            // (works better on larger numbers).
            assert!(burn(token) > 4950, 1);

            test::return_shared(test, pool);
        };
    }

    /// The owner swaps back BEEP for SUI and expects an increase in price.
    /// The sent amount of BEEP is 1000, initial price was 1 BEEP : 1000 SUI;
    fun test_swap_tok_(test: &mut Scenario) {
        test_swap_sui_(test);

        let (owner, _) = people();

        next_tx(test, &owner); {
            let pool = test::take_shared<Pool<POOLEY, BEEP>>(test);
            let pool_mut = test::borrow_mut(&mut pool);

            let sui = pool::swap_token(pool_mut, mint<BEEP>(1000, ctx(test)), ctx(test));

            // Actual win is 1005971, which is ~ 0.6% profit
            assert!(burn(sui) > 1000000u64, 2);

            test::return_shared(test, pool);
        };
    }

    /// The owner tries to withdraw all liquidity from the pool.
    fun test_withdraw_all_(test: &mut Scenario) {
        test_swap_tok_(test);

        let (owner, _) = people();

        next_tx(test, &owner); {
            let lsp = mint<LSP<POOLEY, BEEP>>(1000, ctx(test));
            let pool = test::take_shared<Pool<POOLEY, BEEP>>(test);
            let pool_mut = test::borrow_mut(&mut pool);

            let (sui, tok) = pool::remove_liquidity(pool_mut, lsp, ctx(test));
            let (sui_reserve, tok_reserve, lsp_supply) = pool::get_amounts(pool_mut);

            assert!(sui_reserve == 0, 3);
            assert!(tok_reserve == 0, 3);
            assert!(lsp_supply == 0, 3);

            // make sure that withdrawn assets
            assert!(burn(sui) > 1000000000, 3);
            assert!(burn(tok) < 1000000, 3);

            test::return_shared(test, pool);
        };
    }

    /// This just tests the math.
    fun test_math_(_: &mut Scenario) {
        let u64_max = 18446744073709551615;
        let max_val = u64_max / 10000;

        // Try small values
        assert!(pool::get_input_price(10, 1000, 1000, 0) == 9, 0);

        // Even with 0 comission there's this small loss of 1
        assert!(pool::get_input_price(10000, max_val, max_val, 0) == 9999, 0);
        assert!(pool::get_input_price(1000, max_val, max_val, 0) == 999, 0);
        assert!(pool::get_input_price(100, max_val, max_val, 0) == 99, 0);
    }

    // utilities
    fun scenario(): Scenario { test::begin(&@0x1) }
    fun people(): (address, address) { (@0xBEEF, @0x1337) }
}
