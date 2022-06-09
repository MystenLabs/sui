// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Implementation of a liquidity Pool for Sui.
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

    /// The Capability to create new pools.
    struct PoolCreatorCap has key, store {
        id: VersionedID
    }

    /// The Pool token that will be used to mark the pool share
    /// of a liquidity provider.
    struct LSP<phantom T> has drop {}

    /// The pool with exchange.
    struct Pool<phantom T> has key {
        id: VersionedID,
        sui: Balance<SUI>,
        token: Balance<T>,
        lsp_treasury: TreasuryCap<LSP<T>>
    }

    /// On init creator of the module gets the capability
    /// to create new `Pool`s.
    fun init(ctx: &mut TxContext) {
        Transfer::transfer(PoolCreatorCap {
            id: TxContext::new_id(ctx)
        }, TxContext::sender(ctx))
    }

    /// Create new `Pool` for token `T`. Each Pool holds a `Coin<T>`
    /// and a `Coin<SUI>`. Swaps are available in both directions.
    ///
    /// - `share` argument defines the initial amount of LSP tokens
    /// received by the creator of the Pool.
    public fun create_pool<T>(
        _: &PoolCreatorCap,
        token: Coin<T>,
        sui: Coin<SUI>,
        share: u64,
        ctx: &mut TxContext
    ) {
        let lsp_treasury = Coin::create_currency(LSP<T> {}, ctx);
        let lsp = Coin::mint(share, &mut lsp_treasury, ctx);

        Transfer::transfer(lsp, TxContext::sender(ctx));
        Transfer::share_object(Pool {
            id: TxContext::new_id(ctx),
            token: Coin::into_balance(token),
            sui: Coin::into_balance(sui),
            lsp_treasury,
        });
    }

    /// Swap `Coin<SUI>` for the `Coin<T>`.
    public(script) fun swap_sui<T>(
        pool: &mut Pool<T>, sui: Coin<SUI>, ctx: &mut TxContext
    ) {
        let sui_balance = Coin::into_balance(sui);

        // Calculate the output amount - fee
        let (sui_reserve, token_reserve, _) = get_amounts(pool);
        let output_amount = get_input_price(
            Balance::value(&sui_balance),
            sui_reserve,
            token_reserve
        );

        Balance::join(&mut pool.sui, sui_balance);
        Transfer::transfer(
            Coin::withdraw(&mut pool.token, output_amount, ctx),
            TxContext::sender(ctx)
        )
    }

    /// Swap `Coin<T>` for the `Coin<SUI>`.
    public(script) fun swap_token<T>(
        pool: &mut Pool<T>, token: Coin<T>, ctx: &mut TxContext
    ) {
        let tok_balance = Coin::into_balance(token);

        // Calculate the output amount - fee
        let (sui_reserve, token_reserve, _) = get_amounts(pool);
        let output_amount = get_input_price(
            Balance::value(&tok_balance),
            token_reserve,
            sui_reserve,
        );

        Balance::join(&mut pool.token, tok_balance);
        Transfer::transfer(
            Coin::withdraw(&mut pool.sui, output_amount, ctx),
            TxContext::sender(ctx)
        )
    }

    /// Add liquidity to the `Pool`. Sender needs to provide both
    /// `Coin<SUI>` and `Coin<T>`, and in exchange he gets `Coin<LSP>` -
    /// liquidity provider tokens.
    public(script) fun add_liquidity<T>(
        pool: &mut Pool<T>, sui: Coin<SUI>, token: Coin<T>, ctx: &mut TxContext
    ) {
        let sui_balance = Coin::into_balance(sui);
        let token_balance = Coin::into_balance(token);

        let (sui_amount, _, lsp_supply) = get_amounts(pool);

        let sui_added = Balance::value(&sui_balance);
        let share_minted = (sui_added * lsp_supply) / sui_amount;

        // TODO: Figure out what to do with this buddy.
        // let coin2_added = (share_minted * tok_amount) / lsp_supply;

        Balance::join(&mut pool.sui, sui_balance);
        Balance::join(&mut pool.token, token_balance);

        Transfer::transfer(
            Coin::mint(share_minted, &mut pool.lsp_treasury, ctx),
            TxContext::sender(ctx)
        );
    }

    /// Remove liquidity from the `Pool` by burning `Coin<LSP>`.
    /// Sender gets `Coin<T>` and `Coin<SUI>`.
    public(script) fun remove_liquidity<T>(
        pool: &mut Pool<T>,
        lsp: Coin<LSP<T>>,
        ctx: &mut TxContext
    ) {
        let lsp_amount = Coin::burn(lsp, &mut pool.lsp_treasury);
        let (sui_amt, tok_amt, lsp_supply) = get_amounts(pool);

        let sui_removed = (sui_amt * lsp_amount) / lsp_supply;
        let tok_removed = (tok_amt * lsp_amount) / lsp_supply;

        let sender = TxContext::sender(ctx);

        Transfer::transfer(
            Coin::withdraw(&mut pool.sui, sui_removed, ctx),
            sender
        );

        Transfer::transfer(
            Coin::withdraw(&mut pool.token, tok_removed, ctx),
            sender
        );
    }

    /// Get most used values in a handy way:
    /// - amount of SUI
    /// - amount of token
    /// - total supply of LSP
    public fun get_amounts<T>(pool: &Pool<T>): (u64, u64, u64) {
        (
            Balance::value(&pool.sui),
            Balance::value(&pool.token),
            Coin::total_supply(&pool.lsp_treasury)
        )
    }

    /// Calculate the output amount minus the fee - 0.3%
    fun get_input_price(
        input_amount: u64, input_reserve: u64, output_reserve: u64
    ): u64 {
        let input_amount_with_fee = input_amount * 997; // 0.3% fee
        let numerator = input_amount_with_fee * output_reserve;
        let denominator = (input_reserve * 1000) + input_amount_with_fee;

        numerator / denominator
    }

    #[test_only]
    public fun get_price_for_sui<T>(pool: &Pool<T>, to_sell: u64): u64 {
        let (sui_amt, tok_amt, _) = get_amounts(pool);
        get_input_price(to_sell, tok_amt, sui_amt)
    }

    #[test_only]
    public fun get_price_for_token<T>(pool: &Pool<T>, to_sell: u64): u64 {
        let (sui_amt, tok_amt, _) = get_amounts(pool);
        get_input_price(to_sell, sui_amt, tok_amt)
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) {
        init(ctx)
    }
}

#[test_only]
module defi::pool_tests {
    use sui::SUI::SUI;
    // use sui::Balance::{Self};
    use sui::Coin::{Self, Coin, mint_for_testing as mint, };
    use sui::TestScenario::{Self as test, Scenario, next_tx, ctx};
    use defi::pool::{Self, Pool, LSP};

    // Gonna be our test token.
    struct BEEP {}

    // Tests section
    #[test] public(script) fun test_init_pool() { test_init_pool_(&mut scenario()) }
    #[test] public(script) fun test_swap_sui() { test_swap_sui_(&mut scenario()) }
    #[test] public(script) fun test_swap_tok() { test_swap_tok_(&mut scenario()) }

    /// Init a Pool with a 1_000_000 BEEP and 1_000_000_000 SUI;
    /// Set the ratio BEEP : SUI = 1 : 1000.
    /// Set LSP token amount to 1000;
    public(script) fun test_init_pool_(test: &mut Scenario) {
        let (owner, _) = people();

        next_tx(test, &owner); {
            pool::init_for_testing(ctx(test));
        };

        next_tx(test, &owner); {
            let pool_cap = test::take_owned<pool::PoolCreatorCap>(test);

            pool::create_pool(
                &pool_cap,
                mint<BEEP>(1000000, ctx(test)),
                mint<SUI>(1000000000, ctx(test)),
                1000,
                ctx(test)
            );

            test::return_owned(test, pool_cap);
        };

        next_tx(test, &owner); {
            let lsp = test::take_owned<Coin<LSP<BEEP>>>(test);
            let pool = test::take_shared<Pool<BEEP>>(test);
            let pool_mut = test::borrow_mut(&mut pool);
            let (amt_sui, amt_tok, lsp_supply) = pool::get_amounts(pool_mut);

            assert!(Coin::value(&lsp) == 1000, 0);
            assert!(lsp_supply == 1000, 0);
            assert!(amt_sui == 1000000000, 0);
            assert!(amt_tok == 1000000, 0);

            test::return_owned(test, lsp);
            test::return_shared(test, pool);
        };
    }

    /// The other guy tries to exchange 5_000_000 sui for ~ 5000 BEEP,
    /// minus the commission that is paid to the pool.
    public(script) fun test_swap_sui_(test: &mut Scenario) {
        test_init_pool_(test);

        let (_, the_guy) = people();

        next_tx(test, &the_guy); {
            let pool = test::take_shared<Pool<BEEP>>(test);
            let pool_mut = test::borrow_mut(&mut pool);

            pool::swap_sui(pool_mut, mint<SUI>(5000000, ctx(test)), ctx(test));
            test::return_shared(test, pool);
        };

        // Check the value of the coin received by the guy.
        // Due to rounding problem the value is not precise
        // (better on larger numbers).
        next_tx(test, &the_guy); {
            let beep = test::take_owned<Coin<BEEP>>(test);
            assert!(Coin::value(&beep) > 4950, 1);
            test::return_owned(test, beep);
        };
    }

    /// The owner swaps back BEEP for SUI and expects an increase in price.
    /// The sent amount of BEEP is 1000, initial price was 1 BEEP : 1000 SUI;
    public(script) fun test_swap_tok_(test: &mut Scenario) {
        test_swap_sui_(test);

        let (owner, _) = people();

        next_tx(test, &owner); {
            let pool = test::take_shared<Pool<BEEP>>(test);
            let pool_mut = test::borrow_mut(&mut pool);

            pool::swap_token(pool_mut, mint<BEEP>(1000, ctx(test)), ctx(test));
            test::return_shared(test, pool);
        };

        next_tx(test, &owner); {
            let sui = test::take_owned<Coin<SUI>>(test);

            // Actual win is 1005971, which is ~ 0.6% profit
            assert!(Coin::value(&sui) > 1000000u64, 2);

            test::return_owned(test, sui);
        };
    }

    // utilities
    fun scenario(): Scenario { test::begin(&@0x1) }
    fun people(): (address, address) { (@0xBEEF, @0x1337) }
}
