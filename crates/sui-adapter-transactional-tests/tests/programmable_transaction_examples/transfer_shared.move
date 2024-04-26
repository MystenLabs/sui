// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests shared object transfer as part of programmable transactions

//# init --addresses test=0x0  t2=0x0 --accounts A B

//# publish

module t2::o2 {
    public struct Obj2 has key, store {
        id: UID,
    }

    public entry fun create(ctx: &mut TxContext) {
        let o = Obj2 { id: object::new(ctx) };
        transfer::public_share_object(o)
    }



}

//# run t2::o2::create

//# view-object 2,0

//# programmable --sender A --inputs object(2,0) @B
//> TransferObjects([Input(0)], Input(1))
