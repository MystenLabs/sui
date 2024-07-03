// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses t1=0x0 t2=0x0 --accounts A --shared-object-deletion true

// Merge:
// shared into owned -> Can do anything, SO is deleted
// owned into shared -> SO restrictions apply (DF, DOF, Transfer, Freeze, Delete)
// shared into shared -> SO restrictions apply (DF, DOF, Transfer, Freeze, Delete)

// tfer, then abort

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

    public fun mint_shared_obj(ctx: &mut TxContext) {
        transfer::public_share_object(Obj2 { id: object::new(ctx) });
    }

    public fun mint_owned_coin(ctx: &mut TxContext) {
        transfer::public_transfer(coin::zero<SUI>(ctx), @A)
    }

    public fun deleter(o2: Coin<SUI>) {
        coin::destroy_zero(o2);
    }

    public fun freezer(o2: Coin<SUI>) {
        transfer::public_freeze_object(o2);
    }

    public fun dofer(parent: &mut Obj2, o2: Coin<SUI>) {
        dof::add(&mut parent.id, 0, o2);
    }

    public fun dfer(parent: &mut Obj2, o2: Coin<SUI>) {
        df::add(&mut parent.id, 0, o2);
    }

    public fun transferer(o2: Coin<SUI>) {
        transfer::public_transfer(o2, @0x0);
    }

    public fun sharer(o2: Coin<SUI>) {
        transfer::public_share_object(o2);
    }
}

// **Merge owned into shared**

//# run t2::o2::mint_owned_coin

//# run t2::o2::mint_shared_coin

//# run t2::o2::mint_shared_obj

//# view-object 2,0

//# view-object 3,0

//# view-object 4,0

// Merge and then try to freeze
//# programmable --sender A --inputs object(2,0) object(3,0)
//> 0: MergeCoins(Input(1), [Input(0)]);
//> 1: t2::o2::freezer(Input(1));

// Merge and then try to add as dof
//# programmable --sender A --inputs object(2,0) object(3,0) object(4,0)
//> 0: MergeCoins(Input(1), [Input(0)]);
//> 1: t2::o2::dofer(Input(2), Input(1));

// Merge and then try to add as df
//# programmable --sender A --inputs object(2,0) object(3,0) object(4,0)
//> 0: MergeCoins(Input(1), [Input(0)]);
//> 1: t2::o2::dfer(Input(2), Input(1));

// Merge and then try to transfer it
//# programmable --sender A --inputs object(2,0) object(3,0) object(4,0)
//> 0: MergeCoins(Input(1), [Input(0)]);
//> 1: t2::o2::transferer(Input(1));

// Merge and then try to transfer it with PTB transfer
//# programmable --sender A --inputs object(2,0) object(3,0) object(4,0) @A
//> 0: MergeCoins(Input(1), [Input(0)]);
//> 1: TransferObjects([Input(1)], Input(3));

// **Merge shared into shared**

//# run t2::o2::mint_shared_coin

//# run t2::o2::mint_shared_coin

//# run t2::o2::mint_shared_obj

//# view-object 13,0

//# view-object 14,0

//# view-object 15,0

// Merge and then try to freeze
//# programmable --sender A --inputs object(13,0) object(14,0)
//> 0: MergeCoins(Input(1), [Input(0)]);
//> 1: t2::o2::freezer(Input(1));

// Merge and then try to add as dof
//# programmable --sender A --inputs object(13,0) object(14,0) object(15,0)
//> 0: MergeCoins(Input(1), [Input(0)]);
//> 1: t2::o2::dofer(Input(2), Input(1));

// Merge and then try to add as df
//# programmable --sender A --inputs object(13,0) object(14,0) object(15,0)
//> 0: MergeCoins(Input(1), [Input(0)]);
//> 1: t2::o2::dfer(Input(2), Input(1));

// Merge and then try to transfer it
//# programmable --sender A --inputs object(13,0) object(14,0) object(15,0)
//> 0: MergeCoins(Input(1), [Input(0)]);
//> 1: t2::o2::transferer(Input(1));

// Merge and then try to transfer it with PTB transfer
//# programmable --sender A --inputs object(13,0) object(14,0) object(15,0) @A
//> 0: MergeCoins(Input(1), [Input(0)]);
//> 1: TransferObjects([Input(1)], Input(3));
