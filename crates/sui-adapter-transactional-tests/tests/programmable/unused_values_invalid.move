// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests invalid unused values

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct R has key, store { id: UID }
    public struct Cup<T> has copy, drop { t: T }
    public struct Copyable has copy {}

    public fun r(ctx: &mut TxContext): R {
        R { id: object::new(ctx) }
    }

    public fun cup<T>(t: T): Cup<T> { Cup { t } }
    public fun destroy_cup<T>(cup: Cup<T>): T { let Cup { t } = cup; t }

    public fun copyable(): Copyable { Copyable {} }
    public fun borrow(_: &Copyable) {}
    public fun copy_(c: Copyable) { let Copyable {} = c; }
}

//# programmable --sender A --inputs @A
//> 0: test::m1::r();
//> 1: TransferObjects([Result(0)], Input(0));
//> 2: test::m1::r();

// unconsumed copyable value, and most recent usage was not by-value
//# programmable --sender A
//> 0: test::m1::copyable();
//> 1: test::m1::borrow(Result(0));
//> 2: test::m1::copy_(Result(0));
//> 3: test::m1::borrow(Result(0));
//> 4: test::m1::copy_(Result(0));
//> 5: test::m1::borrow(Result(0));

// unconsumed copyable value, and most recent usage was not by-value
//# programmable --sender A
//> 0: test::m1::copyable();
//> 1: test::m1::cup<test::m1::Copyable>(Result(0));
//> 2: test::m1::destroy_cup<test::m1::Copyable>(Result(1));
