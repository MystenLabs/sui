// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// In Sui mode, `&mut TxContext` and `&TxContext` are exempt from counting as mutating
// parameters for this lint. Functions whose only references are TxContexts are treated
// as pure: discarding their result warns.

module a::tests {
    use sui::tx_context::TxContext;

    fun new_id(_ctx: &mut TxContext): u64 { 0 }
    fun read_only(_ctx: &TxContext): u64 { 0 }
    fun really_mutating(x: &mut u64): u64 { *x = *x + 1; *x }

    // `&mut TxContext` exempt: warn on discard
    fun mut_tx_ctx(ctx: &mut TxContext) {
        new_id(ctx); // warn
    }

    // `&TxContext` exempt: warn on discard
    fun immut_tx_ctx(ctx: &TxContext) {
        read_only(ctx); // warn
    }

    // genuinely mutating: do not warn
    fun mut_other() {
        let y = 0;
        really_mutating(&mut y);
    }
}

module sui::tx_context {
    struct TxContext has drop {}
}
