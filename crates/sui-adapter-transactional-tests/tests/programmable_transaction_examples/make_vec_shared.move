// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests shared object scenarios as part of programmable transactions

//# init --addresses t2=0x0 --accounts A B --shared-object-deletion true

//# publish

module t2::o2 {

    public struct Obj2 has key, store {
        id: UID,
    }

    public entry fun create(ctx: &mut TxContext) {
        let o = Obj2 { id: object::new(ctx) };
        transfer::public_share_object(o)
    }

    public entry fun consume(mut v: vector<Obj2>) {
        while (!vector::is_empty(&v)) {
            let Obj2 { id } = vector::pop_back(&mut v);
            object::delete(id);
        };
        vector::destroy_empty(v)
    }

}

//# run t2::o2::create

//# view-object 2,0

//# programmable --sender A --inputs object(2,0)
//> 0: MakeMoveVec<t2::o2::Obj2>([Input(0)]);
//> t2::o2::consume(Result(0))
