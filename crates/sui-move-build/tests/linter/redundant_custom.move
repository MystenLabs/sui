// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::test {
    use sui::object::UID;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct S1 has key, store {
        id: UID
    }

    #[lint_allow(self_transfer)]
    public fun custom_transfer_bad(o: S1, ctx: &mut TxContext) {
        transfer::transfer(o, tx_context::sender(ctx))
    }

    #[lint_allow(share_owned)]
    public fun custom_share_bad(o: S1) {
        transfer::share_object(o)
    }

    public fun custom_freeze_bad(o: S1) {
        transfer::freeze_object(o)
    }
}
