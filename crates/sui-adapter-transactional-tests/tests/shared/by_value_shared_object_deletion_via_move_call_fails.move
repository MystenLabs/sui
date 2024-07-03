// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses t1=0x0 t2=0x0 --shared-object-deletion true

//# publish

module t2::o2 {
    use sui::dynamic_field as df;
    use sui::dynamic_object_field as dof;
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

    public fun deleter(o2: Obj2) {
        let Obj2 { id } = o2;
        object::delete(id);
    }

    public fun freezer(o2: Obj2) {
        transfer::freeze_object(o2);
    }

    public fun dofer(parent: &mut Obj2, o2: Obj2) {
        dof::add(&mut parent.id, 0, o2);
    }

    public fun dfer(parent: &mut Obj2, o2: Obj2) {
        df::add(&mut parent.id, 0, o2);
    }

    public fun transferer(o2: Obj2) {
        transfer::transfer(o2, @0x0);
    }

    public fun sharer(o2: Obj2) {
        transfer::share_object(o2);
    }

    public fun share_coin(o2: Coin<SUI>) {
        transfer::public_share_object(o2);
    }

    public fun id<T>(i: T): T { i }
}

//# run t2::o2::create

//# run t2::o2::create

//# view-object 2,0

//# view-object 3,0

// pass through a move function and then try to freeze
//# programmable --inputs object(2,0) object(3,0)
//> 0: t2::o2::id<t2::o2::Obj2>(Input(1));
//> 1: t2::o2::freezer(Result(0));

// pass through a move function and then try to add as dof
//# programmable --inputs object(2,0) object(3,0)
//> 0: t2::o2::id<t2::o2::Obj2>(Input(1));
//> 1: t2::o2::dofer(Input(0), Result(0));

// pass through a move function and then try to add as df
//# programmable --inputs object(2,0) object(3,0)
//> 0: t2::o2::id<t2::o2::Obj2>(Input(1));
//> 1: t2::o2::dfer(Input(0), Result(0));

// pass through a move function and then try to transfer it
//# programmable --inputs object(2,0) object(3,0)
//> 0: t2::o2::id<t2::o2::Obj2>(Input(1));
//> 1: t2::o2::transferer(Result(0));

//# run t2::o2::mint_shared_coin

//# view-object 10,0

// Try to double-use the input
//# programmable --inputs 0 object(10,0) @0x0
//> 0: t2::o2::id<sui::coin::Coin<sui::sui::SUI>>(Input(1));
//> 1: SplitCoins(Result(0), [Input(0)]);
//> 2: TransferObjects([Result(1)], Input(2));
//> 3: sui::transfer::public_share_object<sui::coin::Coin<sui::sui::SUI>>(Input(1));

// Try to double-use the input using a user-defined function
//# programmable --inputs 0 object(10,0) @0x0
//> 0: t2::o2::id<sui::coin::Coin<sui::sui::SUI>>(Input(1));
//> 1: SplitCoins(Result(0), [Input(0)]);
//> 2: TransferObjects([Result(1)], Input(2));
//> 3: t2::o2::share_coin(Input(1));

// Try to transfer the shared object and double-use the input
//# programmable --inputs 0 object(10,0) @0x0
//> 0: t2::o2::id<sui::coin::Coin<sui::sui::SUI>>(Input(1));
//> 1: SplitCoins(Result(0), [Input(0)]);
//> 2: TransferObjects([Result(1)], Input(2));
//> 3: TransferObjects([Input(1)], Input(2));

// Try to transfer the shared object
//# programmable --inputs 0 object(10,0) @0x0
//> 0: t2::o2::id<sui::coin::Coin<sui::sui::SUI>>(Input(1));
//> 1: SplitCoins(Result(0), [Input(0)]);
//> 2: TransferObjects([Result(1)], Input(2));
//> 3: TransferObjects([Result(0)], Input(2));
