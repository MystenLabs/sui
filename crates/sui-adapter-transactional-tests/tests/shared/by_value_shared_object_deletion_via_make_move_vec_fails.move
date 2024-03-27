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

    public fun pop_coin(mut o2: vector<Coin<SUI>>): Coin<SUI> {
        let o = vector::pop_back(&mut o2);
        vector::destroy_empty(o2);
        o
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

    public fun freezee(mut o2: vector<Obj2>) {
        let o = vector::pop_back(&mut o2);
        freezer(o);
        vector::destroy_empty(o2);
    }

    public fun dof_(parent: &mut Obj2, mut o2: vector<Obj2>) {
        let o = vector::pop_back(&mut o2);
        dofer(parent, o);
        vector::destroy_empty(o2);
    }

    public fun df_(parent: &mut Obj2, mut o2: vector<Obj2>) {
        let o = vector::pop_back(&mut o2);
        dfer(parent, o);
        vector::destroy_empty(o2);
    }

    public fun transfer_(mut o2: vector<Obj2>) {
        let o = vector::pop_back(&mut o2);
        transferer(o);
        vector::destroy_empty(o2);
    }

    public fun pop_it(mut o2: vector<Obj2>): Obj2 {
        let o = vector::pop_back(&mut o2);
        vector::destroy_empty(o2);
        o
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
}

//# run t2::o2::create

//# run t2::o2::create

//# view-object 2,0

//# view-object 3,0

// Make MoveVec and then try to freeze
//# programmable --inputs object(2,0) object(3,0)
//> 0: MakeMoveVec<t2::o2::Obj2>([Input(1)]);
//> 1: t2::o2::freezee(Result(0));

// Make MoveVec and then try to add as dof
//# programmable --inputs object(2,0) object(3,0)
//> 0: MakeMoveVec<t2::o2::Obj2>([Input(1)]);
//> 1: t2::o2::dof_(Input(0), Result(0));

// Make MoveVec and then try to add as df
//# programmable --inputs object(2,0) object(3,0)
//> 0: MakeMoveVec<t2::o2::Obj2>([Input(1)]);
//> 1: t2::o2::df_(Input(0), Result(0));

// Make MoveVec and then try to transfer it
//# programmable --inputs object(2,0) object(3,0)
//> 0: MakeMoveVec<t2::o2::Obj2>([Input(1)]);
//> 1: t2::o2::transfer_(Result(0));

// Make MoveVec pop and return it, then try to freeze
//# programmable --inputs object(2,0) object(3,0)
//> 0: MakeMoveVec<t2::o2::Obj2>([Input(1)]);
//> 1: t2::o2::pop_it(Result(0));
//> 2: t2::o2::freezer(Result(1));

// Make MoveVec pop and return it, then try to add as dof
//# programmable --inputs object(2,0) object(3,0)
//> 0: MakeMoveVec<t2::o2::Obj2>([Input(1)]);
//> 1: t2::o2::pop_it(Result(0));
//> 2: t2::o2::dofer(Input(0), Result(1));

// Make MoveVec pop and return it, then try to add as df
//# programmable --inputs object(2,0) object(3,0)
//> 0: MakeMoveVec<t2::o2::Obj2>([Input(1)]);
//> 1: t2::o2::pop_it(Result(0));
//> 2: t2::o2::dfer(Input(0), Result(1));

// Make MoveVec pop and return it, then try to transfer it
//# programmable --inputs object(2,0) object(3,0)
//> 0: MakeMoveVec<t2::o2::Obj2>([Input(1)]);
//> 1: t2::o2::pop_it(Result(0));
//> 2: t2::o2::transferer(Result(1));

// Make MoveVec pop and return it, then try to transfer it with PT transfer
//# programmable --inputs object(3,0) @0x0
//> 0: MakeMoveVec<t2::o2::Obj2>([Input(0)]);
//> 1: t2::o2::pop_it(Result(0));
//> 2: TransferObjects([Result(1)], Input(1));

//# run t2::o2::mint_shared_coin

//# view-object 15,0

// This is OK -- split off from a shared object and transfer the split-off coin
// But fails because we need to reshare the shared object
//# programmable --inputs 0 object(15,0) @0x0
//> 0: MakeMoveVec([Input(1)]);
//> 1: t2::o2::pop_coin(Result(0));
//> 2: SplitCoins(Result(1), [Input(0)]);
//> 3: TransferObjects([Result(2)], Input(2));

// Try to call public_share_object directly -- this should fail
//# programmable --inputs 0 object(15,0) @0x0
//> 0: MakeMoveVec([Input(1)]);
//> 1: t2::o2::pop_coin(Result(0));
//> 2: SplitCoins(Result(1), [Input(0)]);
//> 3: TransferObjects([Result(2)], Input(2));
//> 4: sui::transfer::public_share_object(Input(1));

// Try to reshare the shared object -- this should fail since the input was
// used for the `MakeMoveVec` call
//# programmable --inputs 0 object(15,0) @0x0
//> 0: MakeMoveVec([Input(1)]);
//> 1: t2::o2::pop_coin(Result(0));
//> 2: SplitCoins(Result(1), [Input(0)]);
//> 3: TransferObjects([Result(2)], Input(2));
//> 4: t2::o2::share_coin(Input(1));

// Try to transfer the shared object -- this should fail since the input was
// used for the `MakeMoveVec` call
//# programmable --inputs 0 object(15,0) @0x0
//> 0: MakeMoveVec([Input(1)]);
//> 1: t2::o2::pop_coin(Result(0));
//> 2: SplitCoins(Result(1), [Input(0)]);
//> 3: TransferObjects([Result(2)], Input(2));
//> 4: TransferObjects([Input(1)], Input(2));

// Try to transfer the shared object
//# programmable --inputs 0 object(15,0) @0x0
//> 0: MakeMoveVec([Input(1)]);
//> 1: t2::o2::pop_coin(Result(0));
//> 2: SplitCoins(Result(1), [Input(0)]);
//> 3: TransferObjects([Result(2)], Input(2));
//> 4: TransferObjects([Result(1)], Input(2));
