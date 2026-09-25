// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::example;

public struct Sword has key, store {
    id: UID,
    magic: u64,
}

public fun sword_create(magic: u64, ctx: &mut TxContext): Sword {
    Sword {
        id: object::new(ctx),
        magic: magic,
    }
}
