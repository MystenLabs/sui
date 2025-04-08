// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This example demonstrates emitting a random u128 (e.g., for an offchain lottery)
module basics::random;

use sui::{event, random::Random};

public struct RandomU128Event has copy, drop {
    value: u128,
}

entry fun new(r: &Random, ctx: &mut TxContext) {
    let mut gen = r.new_generator(ctx);
    let value = gen.generate_u128();
    event::emit(RandomU128Event { value });
}
