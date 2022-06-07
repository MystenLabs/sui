// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


/// This module aims to showcase AMMs on Sui using the Constant
/// Product Maker Model.
///
/// - The famous: `x * y = k`
/// - Every coin is traded against SUI;
/// - LSP tokens are issued for every provider and can be used to withdraw
/// assets from the pool.
///
/// This example doesn't aim to be a perfect solution for real world
/// applications. For example it does not address the integer rounding
/// problem and hence is vulnerable by default to attacks like this one.
module defi::pool {
    use sui::ID::{VersionedID};
    use sui::Balance::{Self as balance, value, Balance};
    use sui::Coin::{Self as coin, Coin};
    use sui::TxContext::{Self, TxContext};
    use sui::Transfer;
    use sui::SUI::SUI;

    /// The liquidity provider token. Received upon providing
    /// liquidity into the pool.
    ///
    /// Also LSP stands for Lumpy Space Princess ;)
    struct LSP has drop {}

    /// The pool itself that holds the K.
    struct Pool<phantom T> has key {
        id: VersionedID,
        amt_t: Balance<T>,
        amt_s: Balance<SUI>,

        /// The coeffiecient for the pool.
        /// Set to u128 intentionally to eliminate possible
        /// overflow from multiplication of two u64s.
        the_k: u128,

        lsp_minted: u64,
        lsp_treasury_cap: coin::TreasuryCap<LSP>
    }

    /// Get the `K`
    public fun the_k<T>(pool: &Pool<T>): u128 { pool.the_k }

    /// Create a new liquidity pool. It is important that
    /// the creator deposits both currencies therefore setting the
    /// constant K.
    public(script) fun init_pool<T>(
        t: Coin<T>, s: Coin<SUI>, ctx: &mut TxContext
    ) {
        let amt_t = coin::into_balance(t);
        let amt_s = coin::into_balance(s);
        let the_k = (value(&amt_t) as u128) * (value(&amt_s) as u128);

        Transfer::share_object(Pool {
            id: TxContext::new_id(ctx),
            the_k,
            amt_t,
            amt_s,
            lsp_minted: 0,
            lsp_treasury_cap: coin::create_currency(LSP {}, ctx)
        });
    }

    /// Adding liquidity requires depositing an equivalent value of
    /// SUI and token T into the pool.
    ///
    /// The liquidity provider (sender) gets LSP tokens marking his
    /// part of the total pool value.
    public(script) fun add_liquidity<T>(
        pool: &mut Pool<T>, t: Coin<T>, s: Coin<SUI>, ctx: &mut TxContext
    ) {
        let amt_t = coin::into_balance(t);
        let amt_s = coin::into_balance(s);

        assert!(value(&amt_t) > 0, 0);
        assert!(value(&amt_s) > 0, 0);

        // the K is calculated based on multiplication of amounts
        // hence the percentage of investment can be calculated
        // following the same principle.
        //
        // The 1000 multiplier is added just to make calculations more fun,
        // meaning that the loss of data (round problem) on division will be
        // significantly smaller.
        let to_mint: u128 = pool.the_k * 1000
            / (value(&amt_t) as u128)
            / (value(&amt_s) as u128);

        // downcast u128 to more common u64
        let to_mint = (to_mint as u64);

        Std::Debug::print(&to_mint);

        pool.lsp_minted = pool.lsp_minted + to_mint;
        balance::join(&mut pool.amt_t, amt_t);
        balance::join(&mut pool.amt_s, amt_s);

        Transfer::transfer(
            coin::mint(to_mint, &mut pool.lsp_treasury_cap, ctx),
            TxContext::sender(ctx)
        );
    }

    public(script) fun remove_liquidity<T>(
        // pool: &mut Pool<T>, lsp: Coin<LSP>, _ctx: &mut TxContext
    ) {
        // let percent_withdrawn = coin::value(&lsp) * 10000 / pool.lsp_minted; //  /= 10000
        // let amt_t = coin::withdraw(pool.)
    }
}

#[test_only]
module defi::pool_tests {
    use sui::SUI::SUI;
    use sui::Balance::{Self};
    use sui::Coin::{Self as coin, Coin, mint_for_testing as mint, };
    use sui::TestScenario::{Self as test, Scenario, next_tx, ctx};
    use defi::pool::{Self, Pool, LSP};

    // Gonna be our test token.
    struct BEEP {}

    // Tests section
    #[test] public(script) fun test_init_pool() { test_init_pool_(&mut scenario()) }
    #[test] public(script) fun test_add_liquidity() { test_add_liquidity_(&mut scenario()) }

    //
    public(script) fun test_init_pool_(test: &mut Scenario) {
        let (owner, _) = people();

        next_tx(test, &owner); {
            pool::init_pool<BEEP>(
                mint<BEEP>(100, ctx(test)),
                mint<SUI>(100000, ctx(test)),
                ctx(test)
            );
        };

        next_tx(test, &owner); {
            let pool = test::take_shared<Pool<BEEP>>(test);
            let pool_mut = test::borrow_mut(&mut pool);

            assert!(pool::the_k(pool_mut) == 100000 * 100, 0);

            test::return_shared(test, pool);
        };
    }

    public(script) fun test_add_liquidity_(test: &mut Scenario) {
        test_init_pool_(test);

        let (_, the_guy) = people();

        next_tx(test, &the_guy); {
            let pool = test::take_shared<Pool<BEEP>>(test);
            let pool_mut = test::borrow_mut(&mut pool);

            pool::add_liquidity(
                pool_mut,
                mint<BEEP>(50, ctx(test)),
                mint<SUI>(500000, ctx(test)),
                ctx(test)
            );

            test::return_shared(test, pool);
        };

        next_tx(test, &the_guy); {
            let liquidity = test::take_owned<Coin<LSP>>(test);
            let value = Balance::value(coin::balance(&liquidity));

            Std::Debug::print(&value);

            test::return_owned(test, liquidity);
        };
    }

    // utilities
    fun scenario(): Scenario { test::begin(&@0x1) }
    fun people(): (address, address) { (@0xBEEF, @0x1337) }
}
