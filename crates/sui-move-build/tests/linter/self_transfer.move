// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::test {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct S1 has key, store {
        id: UID
    }

    struct S2 has key {
        id: UID
    }

    fun init(ctx: &mut TxContext) {
        transfer::public_transfer(S1 { id: object::new(ctx), }, tx_context::sender(ctx));
        transfer::transfer(S1 { id: object::new(ctx), }, tx_context::sender(ctx));
    }

    public fun public_transfer_bad(ctx: &mut TxContext) {
        transfer::public_transfer(S1 { id: object::new(ctx), }, tx_context::sender(ctx))
    }

    public fun private_transfer_bad(ctx: &mut TxContext) {
        transfer::transfer(S1 { id: object::new(ctx), }, tx_context::sender(ctx))
    }

    public fun private_transfer_no_store_bad(ctx: &mut TxContext) {
        transfer::transfer(S2 { id: object::new(ctx), }, tx_context::sender(ctx))
    }

    // non-linter suppression annotation should not suppress linter warnings
    #[allow(all)]
    public fun transfer_through_assigns_bad(ctx: &mut TxContext) {
        let sender = tx_context::sender(ctx);
        let another_sender = sender;
        transfer::public_transfer(S1 { id: object::new(ctx), }, another_sender)
    }

    public fun transfer_to_param_ok(a: address, ctx: &mut TxContext) {
        transfer::public_transfer(S1 { id: object::new(ctx), }, a);
        transfer::transfer(S1 { id: object::new(ctx), }, a);
    }

    public fun conditional_transfer_ok(b: bool, a: address, ctx: &mut TxContext) {
        let xfer_address = if (b) { a } else { tx_context::sender(ctx) };
        transfer::public_transfer(S1 { id: object::new(ctx), }, xfer_address);
        transfer::transfer(S1 { id: object::new(ctx), }, xfer_address);
    }

    #[lint_allow(self_transfer)]
    public fun public_transfer_bad_suppressed(ctx: &mut TxContext) {
        transfer::public_transfer(S1 { id: object::new(ctx), }, tx_context::sender(ctx))
    }
}
