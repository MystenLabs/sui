// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module unknown::unknown;

use sui::coin::{Coin, TreasuryCap};
use sui::coin_registry;
use sui::dynamic_object_field as dof;

public struct UNKNOWN() has drop;

public struct CapKey() has copy, drop, store;

public struct Treasury has key {
    id: UID,
    cap: Option<TreasuryCap<UNKNOWN>>,
}

fun init(witness: UNKNOWN, ctx: &mut TxContext) {
    let (init, mut treasury_cap) = coin_registry::new_currency_with_otw(
        witness,
        2,
        b"UNKNOWN".to_string(),
        b"Unknown".to_string(),
        b"A fake unknown treasury coin for test purposes".to_string(),
        b"https://example.com/unknown.png".to_string(),
        ctx,
    );

    let coin = treasury_cap.mint(1_000_000_000, ctx);
    let metadata_cap = init.finalize(ctx);

    transfer::public_transfer(coin, ctx.sender());
    transfer::share_object(wrap(treasury_cap, ctx));
    transfer::public_transfer(metadata_cap, @0x0);
}

fun wrap(cap: TreasuryCap<UNKNOWN>, ctx: &mut TxContext): Treasury {
    let mut treasury = Treasury {
        id: object::new(ctx),
        cap: option::none(),
    };

    dof::add(&mut treasury.id, CapKey(), cap);
    treasury
}

entry fun show(treasury: &mut Treasury) {
  let cap = treasury.cap.extract();
  dof::add(&mut treasury.id, CapKey(), cap);
}

entry fun hide(treasury: &mut Treasury) {
  let cap: TreasuryCap<UNKNOWN> = dof::remove(&mut treasury.id, CapKey());
  treasury.cap.fill(cap);
}

entry fun burn(treasury: &mut Treasury, coin: Coin<UNKNOWN>) {
  let cap = if (treasury.cap.is_some()) {
      treasury.cap.borrow_mut()
  } else {
      dof::borrow_mut(&mut treasury.id, CapKey())
  };

  cap.burn(coin);
}
