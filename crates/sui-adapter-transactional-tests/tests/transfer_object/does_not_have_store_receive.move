// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests `public_receive` should fail for an object _without_ public transfer,
// and that we cannot directly call `receive` from a PTB.

//# init --accounts A B --addresses test=0x0

//# publish
module test::m {
    use sui::transfer::Receiving;

    public struct Parent has key { id: UID }
    public struct S has key { id: UID }
    public struct Cup<phantom T> has key { id: UID }
    public struct Store has key, store { id: UID }

    public fun mint_s(ctx: &mut TxContext) {
        let id = object::new(ctx);
        let parent = Parent { id };
        let p_address = object::id_address(&parent);
        transfer::transfer(parent, tx_context::sender(ctx));

        let id = object::new(ctx);
        transfer::transfer(S { id }, p_address);
    }

    public fun mint_cup<T>(ctx: &mut TxContext) {
        let id = object::new(ctx);
        let parent = Parent { id };
        let p_address = object::id_address(&parent);
        transfer::transfer(parent, tx_context::sender(ctx));

        let id = object::new(ctx);
        transfer::transfer(Cup<T> { id }, p_address);
    }

    public fun mint_store(ctx: &mut TxContext) {
        let id = object::new(ctx);
        let parent = Parent { id };
        let p_address = object::id_address(&parent);
        transfer::transfer(parent, tx_context::sender(ctx));

        let id = object::new(ctx);
        transfer::transfer(Store { id }, p_address);
    }

    public fun receive_s(parent: &mut Parent, x: Receiving<S>): S {
        let s = transfer::receive(&mut parent.id, x);
        s
    }

    public fun receive_cup<T>(parent: &mut Parent, x: Receiving<Cup<T>>): Cup<T> {
        let s = transfer::receive(&mut parent.id, x);
        s
    }

    public fun parent_uid(p: Parent): UID {
        let Parent { id } = p;
        id
    }

    public fun destroy_s(s: S) {
        let S { id } = s;
        object::delete(id);
    }

    public fun destroy_cup<T>(c: Cup<T>) {
        let Cup { id } = c;
        object::delete(id);
    }

    public fun destroy_store(s: Store) {
        let Store { id } = s;
        object::delete(id);
    }
}

//# run test::m::mint_s --sender A

//# view-object 2,0

//# view-object 2,1

//# programmable --sender A --inputs object(2,0) receiving(2,1)
//> 0: test::m::receive_s(Input(0), Input(1));
//> 1: test::m::destroy_s(Result(0));

//# run test::m::mint_cup --sender A --type-args u64

//# view-object 6,0

//# view-object 6,1

//# programmable --sender A --inputs object(6,1) receiving(6,0)
//> 0: test::m::receive_cup<u64>(Input(0), Input(1));
//> 1: test::m::destroy_cup<u64>(Result(0));

// Try to directly call `public_receive` and `receive` on an object without public transfer.

//# run test::m::mint_s --sender A

//# view-object 10,0

//# view-object 10,1

//# programmable --sender A --inputs object(10,0) receiving(10,1)
//> 0: test::m::parent_uid(Input(0));
//> 1: sui::transfer::public_receive<test::m::S>(Result(0), Input(1));

//# programmable --sender A --inputs object(10,0) receiving(10,1)
//> 0: test::m::parent_uid(Input(0));
//> 1: sui::transfer::receive<test::m::S>(Result(0), Input(1));

// Now publish one with store. We should:
// 1. Not be able to call `receive` to receive it.
// 2. Be able to call `public_receive` to receive it.

//# run test::m::mint_store --sender A

//# view-object 15,0

//# view-object 15,1

//# programmable --sender A --inputs object(15,0) receiving(15,1)
//> 0: test::m::parent_uid(Input(0));
//> 1: sui::transfer::receive<test::m::Store>(Result(0), Input(1));

// Can receive it via a direct `public_receive` call since `Store` has the `store` ability.
//# programmable --sender A --inputs object(15,0) receiving(15,1)
//> 0: test::m::parent_uid(Input(0));
//> 1: sui::transfer::public_receive<test::m::Store>(Result(0), Input(1));
//> 2: test::m::destroy_store(Result(1));
//> 3: sui::object::delete(Result(0));
