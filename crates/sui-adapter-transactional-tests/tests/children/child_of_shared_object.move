// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses T1=0x0 T2=0x0 T3=0x0 A=0x42

//# publish

module T3::O3 {
    use sui::id::VersionedID;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct O3 has key, store {
        id: VersionedID,
    }

    public entry fun create(ctx: &mut TxContext) {
        let o = O3 { id: tx_context::new_id(ctx) };
        transfer::transfer(o, tx_context::sender(ctx))
    }
}

//# publish

module T2::O2 {
    use sui::id::VersionedID;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use T3::O3::O3;

    struct O2 has key, store {
        id: VersionedID,
    }

    public entry fun create_shared(child: O3, ctx: &mut TxContext) {
        transfer::share_object(new(child, ctx))
    }

    public entry fun create_owned(child: O3, ctx: &mut TxContext) {
        transfer::transfer(new(child, ctx), tx_context::sender(ctx))
    }

    public entry fun use_o2_o3(_o2: &mut O2, o3: O3, ctx: &mut TxContext) {
        transfer::transfer(o3, tx_context::sender(ctx))
    }

    fun new(child: O3, ctx: &mut TxContext): O2 {
        let id = tx_context::new_id(ctx);
        transfer::transfer_to_object_id(child, &id);
        O2 { id }
    }
}


//# publish

module T1::O1 {
    use sui::id::VersionedID;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use T2::O2::O2;
    use T3::O3::O3;

    struct O1 has key {
        id: VersionedID,
    }

    public entry fun create_shared(child: O2, ctx: &mut TxContext) {
        transfer::share_object(new(child, ctx))
    }

    // This function will be invalid if _o2 is a shared object and owns _o3.
    public entry fun use_o2_o3(_o2: &mut O2, o3: O3, ctx: &mut TxContext) {
        transfer::transfer(o3, tx_context::sender(ctx))
    }

    fun new(child: O2, ctx: &mut TxContext): O1 {
        let id = tx_context::new_id(ctx);
        transfer::transfer_to_object_id(child, &id);
        O1 { id }
    }
}

//# run T3::O3::create

//# run T2::O2::create_shared --args object(109)

// This run should error as O2/O3 were not defined in O1
//# run T1::O1::use_o2_o3 --args object(111) object(109)

//# run T2::O2::use_o2_o3 --args object(111) object(109)
