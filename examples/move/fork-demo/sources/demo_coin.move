// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module fork_demo::demo_coin {
    use sui::coin::{Self, TreasuryCap};
    use sui::dynamic_field as df;

    public struct DEMO_COIN has drop {}
    public struct DEMO_STATE has key, store {
        id: UID,
        counter: u64,
    }
    public struct DEMO_DYNAMIC has store {
        counter: u64,
    }

    #[allow(deprecated_usage)]
    fun init(witness: DEMO_COIN, ctx: &mut TxContext) {
        let (treasury, metadata) = coin::create_currency(
            witness,
            6,
            b"DEMO",
            b"Demo Coin",
            b"A demo coin for testing fork functionality",
            option::none(),
            ctx
        );
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury, ctx.sender());

        let demo_state = DEMO_STATE {
            id: object::new(ctx),
            counter: 0,
        };
        transfer::public_share_object(demo_state);
    }

    public fun mint(
        treasury: &mut TreasuryCap<DEMO_COIN>,
        amount: u64,
        recipient: address,
        ctx: &mut TxContext
    ) {
        let coin = coin::mint(treasury, amount, ctx);
        transfer::public_transfer(coin, recipient);
    }

    public fun add_demo_dynamic(demo_state: &mut DEMO_STATE) {
        let object = DEMO_DYNAMIC {
            counter: demo_state.counter,
        };
        demo_state.counter = demo_state.counter + 1;
        df::add(&mut demo_state.id, object.counter, object);
    }

    public fun get_demo_counter(demo_state: &DEMO_STATE): u64 {
        demo_state.counter
    }

    public fun get_demo_dynamic_counter(demo_dynamic: &DEMO_DYNAMIC): u64 {
        demo_dynamic.counter
    }

    public fun borrow_demo_dynamic(demo_state: &DEMO_STATE, key: u64): &DEMO_DYNAMIC {
        df::borrow<u64, DEMO_DYNAMIC>(&demo_state.id, key)
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) {
        init(DEMO_COIN {}, ctx);
    }
}
