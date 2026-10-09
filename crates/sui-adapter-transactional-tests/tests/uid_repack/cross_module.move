// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests a UID leaving its defining module and being packed by another module or package,
// including passing the UID between PTB commands.

//# init --addresses ex=0x0 other=0x0 --accounts A --flavor core

//# publish
module ex::a {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    public struct A has key { id: UID }

    public fun mint(ctx: &mut TxContext) {
        transfer::transfer(A { id: object::new(ctx) }, tx_context::sender(ctx))
    }

    public fun into_uid(a: A): UID {
        let A { id } = a;
        id
    }
}

module ex::c {
    use sui::object::UID;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    public struct C has key, store { id: UID }

    public fun from_a(a: ex::a::A, ctx: &TxContext) {
        transfer::transfer(C { id: ex::a::into_uid(a) }, tx_context::sender(ctx))
    }

    public fun from_uid(id: UID): C {
        C { id }
    }

    public fun into_uid(c: C): UID {
        let C { id } = c;
        id
    }
}

//# publish --dependencies ex
module other::d {
    use sui::object::UID;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    public struct D has key { id: UID }

    public fun from_uid(id: UID, ctx: &TxContext) {
        transfer::transfer(D { id }, tx_context::sender(ctx))
    }
}

//# run ex::a::mint --sender A

//# run ex::c::from_a --args object(3,0) --sender A

//# view-object 3,0

//# programmable --inputs object(3,0) @A --sender A
//> 0: ex::c::into_uid(Input(0));
//> 1: ex::c::from_uid(Result(0));
//> TransferObjects([Result(1)], Input(1))

//# view-object 3,0

//# programmable --inputs object(3,0) --sender A
//> 0: ex::c::into_uid(Input(0));
//> other::d::from_uid(Result(0))

//# view-object 3,0
