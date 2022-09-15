// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::item {
    use sui::transfer;
    use sui::object::{Self, UID};
    use std::string::{Self, String};
    use sui::tx_context::{Self, TxContext};

    /// Type that marks Capability to create new `Item`s.
    struct AdminCap has key { id: UID }

    /// Custom NFT-like type.
    struct Item has key, store { id: UID, name: String }

    /// Module initializer is called once on module publish.
    /// Here we create only one instance of `AdminCap` and send it to the publisher.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(AdminCap {
            id: object::new(ctx)
        }, tx_context::sender(ctx))
    }

    /// The entry function can not be called if `AdminCap` is not passed as
    /// the first argument. Hence only owner of the `AdminCap` can perform
    /// this action.
    public entry fun create_and_send(
        _: &AdminCap, name: vector<u8>, to: address, ctx: &mut TxContext
    ) {
        transfer::transfer(Item {
            id: object::new(ctx),
            name: string::utf8(name)
        }, to)
    }
}
