// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example of objects that can be combined to create
/// new objects
module basics::sandwich {
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID};
    use sui::sui::SUI;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct Ham has key, store {
        id: UID
    }

    struct Bread has key, store {
        id: UID
    }

    struct Sandwich has key, store {
        id: UID,
    }

    // This Capability allows the owner to withdraw profits
    struct GroceryOwnerCapability has key {
        id: UID
    }

    // Grocery is created on module init
    struct Grocery has key {
        id: UID,
        profits: Balance<SUI>
    }

    /// Price for ham
    const HAM_PRICE: u64 = 10;
    /// Price for bread
    const BREAD_PRICE: u64 = 2;

    /// Not enough funds to pay for the good in question
    const EInsufficientFunds: u64 = 0;
    /// Nothing to withdraw
    const ENoProfits: u64 = 1;

    #[allow(unused_function)]
    /// On module init, create a grocery
    fun init(ctx: &mut TxContext) {
        transfer::share_object(Grocery {
            id: object::new(ctx),
            profits: balance::zero<SUI>()
        });

        transfer::transfer(GroceryOwnerCapability {
            id: object::new(ctx)
        }, tx_context::sender(ctx));
    }

    /// Exchange `c` for some ham
    public fun buy_ham(
        grocery: &mut Grocery,
        c: Coin<SUI>,
        ctx: &mut TxContext
    ): Ham {
        let b = coin::into_balance(c);
        assert!(balance::value(&b) == HAM_PRICE, EInsufficientFunds);
        balance::join(&mut grocery.profits, b);
        Ham { id: object::new(ctx) }
    }

    /// Exchange `c` for some bread
    public fun buy_bread(
        grocery: &mut Grocery,
        c: Coin<SUI>,
        ctx: &mut TxContext
    ): Bread {
        let b = coin::into_balance(c);
        assert!(balance::value(&b) == BREAD_PRICE, EInsufficientFunds);
        balance::join(&mut grocery.profits, b);
        Bread { id: object::new(ctx) }
    }

    /// Combine the `ham` and `bread` into a delicious sandwich
    public fun make_sandwich(
        ham: Ham, bread: Bread, ctx: &mut TxContext
    ): Sandwich {
        let Ham { id: ham_id } = ham;
        let Bread { id: bread_id } = bread;
        object::delete(ham_id);
        object::delete(bread_id);
        Sandwich { id: object::new(ctx) }
    }

    /// See the profits of a grocery
    public fun profits(grocery: &Grocery): u64 {
        balance::value(&grocery.profits)
    }

    /// Owner of the grocery can collect profits by passing his capability
    public fun collect_profits(_cap: &GroceryOwnerCapability, grocery: &mut Grocery, ctx: &mut TxContext): Coin<SUI> {
        let amount = balance::value(&grocery.profits);

        assert!(amount > 0, ENoProfits);

        // Take a transferable `Coin` from a `Balance`
        coin::take(&mut grocery.profits, amount, ctx)
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) {
        init(ctx);
    }
}

#[test_only]
module basics::test_sandwich {
    use basics::sandwich::{Self, Grocery, GroceryOwnerCapability};
    use sui::test_scenario;
    use sui::coin::{Self};
    use sui::sui::SUI;
    use sui::transfer;
    use sui::test_utils;
    use sui::tx_context;

    #[test]
    fun test_make_sandwich() {
        let owner = @0x1;
        let the_guy = @0x2;

        let scenario_val = test_scenario::begin(owner);
        let scenario = &mut scenario_val;
        test_scenario::next_tx(scenario, owner);
        {
            sandwich::init_for_testing(test_scenario::ctx(scenario));
        };

        test_scenario::next_tx(scenario, the_guy);
        {
            let grocery_val = test_scenario::take_shared<Grocery>(scenario);
            let grocery = &mut grocery_val;
            let ctx = test_scenario::ctx(scenario);

            let ham = sandwich::buy_ham(
                grocery,
                coin::mint_for_testing<SUI>(10, ctx),
                ctx
            );

            let bread = sandwich::buy_bread(
                grocery,
                coin::mint_for_testing<SUI>(2, ctx),
                ctx
            );
            let sandwich = sandwich::make_sandwich(ham, bread, ctx);

            test_scenario::return_shared( grocery_val);
            transfer::public_transfer(sandwich, tx_context::sender(ctx))
        };

        test_scenario::next_tx(scenario, owner);
        {
            let grocery_val = test_scenario::take_shared<Grocery>(scenario);
            let grocery = &mut grocery_val;
            let capability = test_scenario::take_from_sender<GroceryOwnerCapability>(scenario);

            assert!(sandwich::profits(grocery) == 12, 0);
            let profits = sandwich::collect_profits(&capability, grocery, test_scenario::ctx(scenario));
            assert!(sandwich::profits(grocery) == 0, 0);

            test_scenario::return_to_sender(scenario, capability);
            test_scenario::return_shared(grocery_val);
            test_utils::destroy(profits)
        };
        test_scenario::end(scenario_val);
    }
}
