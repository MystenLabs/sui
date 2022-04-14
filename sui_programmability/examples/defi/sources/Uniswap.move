// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A Uniswap-like Marketplace where people can exchange currency X for currency Y.
#[test_only]
module DeFi::UniswapTests {

    use Sui::TestScenario::{Self as TS, ctx, next_tx as tx, Scenario};
    use Sui::ID::VersionedID;
    use Sui::Coin::{Self, Coin};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};
    use Sui::Math;

    const ENOT_SOMETHING: u64 = 404;
    const EWeeWant: u64 = 100;
    const NotFound: u64 = 100;

    /// Top level Market which holds a pair of currencies: C1 and C2.
    struct Market<phantom C1, phantom C2> has key {
        id: VersionedID,
        reserveX: Coin<C1>,
        reserveY: Coin<C2>,
        coefficient: u64
    }

    /// Boo Dee Doo, Beep Boop Boooop.
    public fun create<C1, C2>(
        reserveX: Coin<C1>,
        reserveY: Coin<C2>,
        ctx: &mut TxContext,
    ) {
        let id = TxContext::new_id(ctx);
        let coefficient = Coin::value(&reserveX) * Coin::value(&reserveY);

        Transfer::share_object(Market<C1, C2> {
            id,
            reserveX,
            reserveY,
            coefficient,
        })
    }

    public fun add_liquidity<C1, C2>() {
        // mint liquidity tokens
        // _update() to update reserves
    }

    public fun remove_liquidity<C1, C2>() {
        // burn liquidity tokens
        // _update() to update reserves
    }

    public fun swap<C1, C2>() {
        // swap tokens using 
    }

    struct Coin1 {}
    struct Coin2 {}

    /// Const function to return admin of the contract.
    fun admin(): address { @0xB055 }
    fun user1(): address { @0xA }
    fun user2(): address { @0xB }
    
    #[test]
    fun test_creation() {
        let scenario = &mut TS::begin(&admin());
        
        setup(scenario);
    }

    #[test_only]
    fun setup(scenario: &mut Scenario) {
        tx(scenario, &admin());
        
        let reserveX = mint_coin<Coin1>(scenario);
        let reserveY = mint_coin<Coin2>(scenario);

        create(reserveX, reserveY, ctx(scenario));
    }

    #[test_only]
    fun mint_coin<T>(scenario: &mut Scenario): Coin<T> {
        Coin::mint_for_testing<T>(100000, TS::ctx(scenario))
    }
}
