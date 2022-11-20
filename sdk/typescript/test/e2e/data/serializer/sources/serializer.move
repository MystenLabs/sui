// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module serializer::serializer_tests {
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;

    public entry fun list<T: key + store, C>(
        item: T,
        ctx: &mut TxContext
    ) {
        transfer::transfer(item, tx_context::sender(ctx))
    }
}