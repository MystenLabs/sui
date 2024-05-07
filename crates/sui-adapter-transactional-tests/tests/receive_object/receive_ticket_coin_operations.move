// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses tto=0x0

//# publish
module tto::M1 {
    public struct A has key, store {
        id: UID,
    }

    public struct B has key, store {
        id: UID,
    }

    public fun start(ctx: &mut TxContext) {
        let a = A { id: object::new(ctx) };
        let a_address = object::id_address(&a);
        let b = B { id: object::new(ctx) };
        transfer::public_transfer(a, tx_context::sender(ctx));
        transfer::public_transfer(b, a_address);
    }
}

//# run tto::M1::start

//# view-object 2,0

//# view-object 2,1

// Can't transfer a receiving argument
//# programmable --inputs receiving(2,1) @tto
//> TransferObjects([Input(0)], Input(2))

//# programmable --inputs receiving(2,1) 10
//> SplitCoins(Input(0), [Input(1)])

//# programmable --inputs object(2,0) receiving(2,1)
//> MergeCoins(Input(0), [Input(1)])

//# programmable --inputs object(2,0) receiving(2,1)
//> MergeCoins(Input(1), [Input(0)])

//# view-object 2,0

//# view-object 2,1
