// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// With `enable_coin_regulation` off, the FreezeList cannot be used as an input.

//# init --accounts A --addresses legacy=0x0 --disable-feature-flags enable_coin_regulation

//# publish --sender A
module legacy::legacy_coin;

use sui::coin_registry;

public struct LEGACY_COIN has drop {}

#[allow(deprecated_usage)]
fun init(otw: LEGACY_COIN, ctx: &mut TxContext) {
    let (mut currency, treasury_cap) = coin_registry::new_currency_with_otw(
        otw,
        9,
        b"LC".to_string(),
        b"LEGACY_COIN".to_string(),
        b"".to_string(),
        b"".to_string(),
        ctx,
    );
    let deny_cap = currency.make_regulated(true, ctx);
    let metadata_cap = currency.finalize(ctx);
    transfer::public_transfer(deny_cap, ctx.sender());
    transfer::public_transfer(metadata_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
}

//# run sui::coin_regulation::is_global_pause_enabled --args immshared(0xcf) --type-args legacy::legacy_coin::LEGACY_COIN --sender A
