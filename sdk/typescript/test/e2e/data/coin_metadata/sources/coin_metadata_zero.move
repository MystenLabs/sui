// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module coin_metadata::test_zero;

use sui::coin;
use sui::url;

public struct TEST_ZERO has drop {}

fun init(witness: TEST_ZERO, ctx: &mut TxContext) {
    let (treasury_cap, metadata) = coin::create_currency<TEST_ZERO>(
        witness,
        2,
        b"TEST",
        b"Test Coin",
        b"Test coin metadata",
        option::some(url::new_unsafe_from_bytes(b"http://sui.io")),
        ctx,
    );

    transfer::public_share_object(metadata);
    transfer::public_share_object(treasury_cap)
}
