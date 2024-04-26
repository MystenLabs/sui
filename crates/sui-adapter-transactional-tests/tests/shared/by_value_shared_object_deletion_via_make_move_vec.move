// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses t1=0x0 t2=0x0 --shared-object-deletion true

//# publish

module t2::o2 {
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};

    public struct Obj2 has key, store {
        id: UID,
    }

    public fun mint_shared_coin(ctx: &mut TxContext) {
        transfer::public_share_object(coin::zero<SUI>(ctx))
    }

    public fun create(ctx: &mut TxContext) {
        let o = Obj2 { id: object::new(ctx) };
        transfer::public_share_object(o)
    }

    public fun delete(mut o2: vector<Obj2>) {
        let o = vector::pop_back(&mut o2);
        deleter(o);
        vector::destroy_empty(o2);
    }

    public fun deleter(o2: Obj2) {
        let Obj2 { id } = o2;
        object::delete(id);
    }

    public fun pop_coin(mut o2: vector<Coin<SUI>>): Coin<SUI> {
        let o = vector::pop_back(&mut o2);
        vector::destroy_empty(o2);
        o
    }

    public fun share_coin(o2: Coin<SUI>) {
        transfer::public_share_object(o2);
    }
}

//# run t2::o2::create

//# run t2::o2::create

//# view-object 2,0

//# view-object 3,0

// Make MoveVec and then delete
//# programmable --inputs object(2,0) object(3,0)
//> 0: MakeMoveVec<t2::o2::Obj2>([Input(1)]);
//> 1: t2::o2::delete(Result(0));

//# run t2::o2::mint_shared_coin

//# view-object 7,0

// Reshare the shared object after making the move v
//# programmable --inputs 0 object(7,0) @0x0
//> 0: MakeMoveVec([Input(1)]);
//> 1: t2::o2::pop_coin(Result(0));
//> 2: SplitCoins(Result(1), [Input(0)]);
//> 3: TransferObjects([Result(2)], Input(2));
//> 4: t2::o2::share_coin(Result(1));
