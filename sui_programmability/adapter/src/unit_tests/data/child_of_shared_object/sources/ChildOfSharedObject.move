// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Test::O1 {
    use Sui::ID::VersionedID;
    use Sui::Transfer::{Self, ChildRef};
    use Sui::TxContext::{Self, TxContext};
    use Test::O2::O2;
    use Test::O3::O3;

    struct O1 has key {
        id: VersionedID,
        child: ChildRef<O2>,
    }

    public(script) fun create_shared(child: O2, ctx: &mut TxContext) {
        Transfer::share_object(new(child, ctx))
    }

    // This function will be invalid if _o2 is a shared object and owns _o3.
    public(script) fun use_o2_o3(_o2: &mut O2, _o3: &mut O3, _ctx: &mut TxContext) {}

    fun new(child: O2, ctx: &mut TxContext): O1 {
        let id = TxContext::new_id(ctx);
        let (id, child) = Transfer::transfer_to_object_id(child, id);
        O1 { id, child }
    }
}

module Test::O2 {
    use Sui::ID::VersionedID;
    use Sui::Transfer::{Self, ChildRef};
    use Sui::TxContext::{Self, TxContext};
    use Test::O3::O3;

    struct O2 has key {
        id: VersionedID,
        child: ChildRef<O3>,
    }

    public(script) fun create_shared(child: O3, ctx: &mut TxContext) {
        Transfer::share_object(new(child, ctx))
    }

    public(script) fun create_owned(child: O3, ctx: &mut TxContext) {
        Transfer::transfer(new(child, ctx), TxContext::sender(ctx))
    }

    public(script) fun use_o2_o3(_o2: &mut O2, _o3: &mut O3, _ctx: &mut TxContext) {}

    fun new(child: O3, ctx: &mut TxContext): O2 {
        let id = TxContext::new_id(ctx);
        let (id, child) = Transfer::transfer_to_object_id(child, id);
        O2 { id, child }
    }
}

module Test::O3 {
    use Sui::ID::VersionedID;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    struct O3 has key {
        id: VersionedID,
    }

    public(script) fun create(ctx: &mut TxContext) {
        let o = O3 { id: TxContext::new_id(ctx) };
        Transfer::transfer(o, TxContext::sender(ctx))
    }
}
