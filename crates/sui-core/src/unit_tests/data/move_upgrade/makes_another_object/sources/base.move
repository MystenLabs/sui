// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module base_addr::base {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::event;

    public struct A<T> {
        f1: bool,
        f2: T
    }

    public struct B has key {
        id: UID,
        x: u64,
    }

    public struct BModEvent has copy, drop {
        old: u64,
        new: u64,
    }

    public struct C has key {
        id: UID,
        x: u64,
    }

    public struct CModEvent has copy, drop {
        old: u64,
        new: u64,
    }

    public fun return_0(): u64 { abort 42 }

    public fun plus_1(x: u64): u64 { x + 1 }

    public(package) fun friend_fun(x: u64): u64 { x }

    fun non_public_fun(y: bool): u64 { if (y) 0 else 1 }

    entry fun makes_b(ctx: &mut TxContext) {
        transfer::transfer(
            B { id: object::new(ctx), x: 42 },
            tx_context::sender(ctx),
        )
    }

    entry fun destroys_b(b: B) {
        let B { id, x: _ }  = b;
        object::delete(id);
    }

    entry fun modifies_b(mut b: B, ctx: &mut TxContext) {
        event::emit(BModEvent{ old: b.x, new: 7 });
        b.x = 7;
        transfer::transfer(b, tx_context::sender(ctx))
    }

    entry fun makes_c(ctx: &mut TxContext) {
        transfer::transfer(
            C { id: object::new(ctx), x: 42 },
            tx_context::sender(ctx),
        )
    }

    entry fun modifies_c(mut c: C, ctx: &mut TxContext) {
        event::emit(CModEvent{ old: c.x, new: 7 });
        c.x = 7;
        transfer::transfer(c, tx_context::sender(ctx))
    }


}
