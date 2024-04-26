// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests unused values

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct R has key, store { id: UID }
    public struct Droppable has drop {}
    public struct Cup<T> has copy, drop { t: T }
    public struct Copyable has copy {}

    public fun r(ctx: &mut TxContext): R {
        R { id: object::new(ctx) }
    }

    public fun droppable(): Droppable { Droppable {} }

    public fun cup<T>(t: T): Cup<T> { Cup { t } }

    public fun copyable(): Copyable { Copyable {} }
    public fun borrow(_: &Copyable) {}
    public fun copy_(c: Copyable) { let Copyable {} = c; }

    public fun num_mut(_: &u64) {}
}

// unused object
//# programmable --sender A --inputs @A
//> 0: test::m1::r();
//> TransferObjects([Result(0)], Input(0))

// unused inputs and unused objects and unused results of various kinds
//# programmable --sender A --inputs object(2,0) 0 vector[@0,@0]
//> 0: test::m1::droppable();
//> 1: test::m1::droppable();
//> 2: test::m1::cup<test::m1::Droppable>(Result(0));

// unconsumed copyable value, but most recent usage was by-value
//# programmable --sender A
//> 0: test::m1::copyable();
//> 1: test::m1::borrow(Result(0));
//> 2: test::m1::copy_(Result(0));
//> 3: test::m1::borrow(Result(0));
//> 4: test::m1::copy_(Result(0));

// unused pure that was cast
//# programmable --sender A --inputs 0
//> test::m1::num_mut(Input(0))
