// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module basics::random;

use sui::event;
use sui::random::Random;

public struct RandomU128Event has copy, drop {
    value: u128,
}

entry fun new(r: &Random, ctx: &mut TxContext) {
    let mut gen = r.new_generator(ctx);
    let value = gen.generate_u128();
    event::emit(RandomU128Event { value });
}
