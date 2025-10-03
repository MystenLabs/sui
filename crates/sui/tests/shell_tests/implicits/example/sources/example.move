// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::example;

public struct Sword has key, store {
    id: UID,
    magic: u64,
    strength: u64,
}

// Part 5: Public/entry functions (introduced later in the tutorial)
// docs::#first-pause
public fun sword_create(magic: u64, strength: u64, ctx: &mut TxContext): Sword {
    // Create a sword
    Sword {
        id: object::new(ctx),
        magic: magic,
        strength: strength,
    }
}
