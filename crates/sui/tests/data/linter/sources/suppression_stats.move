// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This file is used to test linter suppression stats output (the test itself is part of CLI tests
// in the sui crate)

#[allow(lint(custom_state_change))]
module linter::suppression_stats {
    use sui::object::UID;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    #[allow(unused_field)]
    public struct S1 has key, store {
        id: UID
    }

    #[allow(lint(self_transfer))]
    public fun custom_transfer_bad(o: S1, ctx: &mut TxContext) {
        transfer::transfer(o, tx_context::sender(ctx))
    }

    #[allow(lint(share_owned))]
    public fun custom_share_bad(o: S1) {
        transfer::share_object(o)
    }

    public fun custom_freeze_bad(o: S1) {
        transfer::freeze_object(o)
    }
}
