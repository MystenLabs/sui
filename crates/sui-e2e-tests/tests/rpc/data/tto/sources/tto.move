// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module tto::M1;

use sui::coin::Coin;
use sui::sui::SUI;
use sui::transfer::Receiving;

public struct A has key, store {
    id: UID,
}

public fun start(coin: Coin<SUI>, ctx: &mut TxContext) {
    let a = A { id: object::new(ctx) };
    let a_address = object::id_address(&a);

    transfer::public_transfer(a, ctx.sender());
    transfer::public_transfer(coin, a_address);
}

public fun receive(parent: &mut A, x: Receiving<Coin<SUI>>) {
    let coin = transfer::public_receive(&mut parent.id, x);
    transfer::public_transfer(coin, @tto);
}

public fun dont_receive(parent: &mut A, _x: Receiving<Coin<SUI>>) {
}
