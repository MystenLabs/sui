// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Protocol-level address freezing, coin pausing, and seizing for regulated coins.
module sui::coin_regulation;

#[error(code = 0)]
const ENotSystemAddress: vector<u8> = b"Only the system can create the freeze list.";

/// Singleton shared object holding the freeze and pause state of regulated coins.
public struct FreezeList has key {
    id: UID,
}

#[allow(unused_function)]
/// Create and share the `FreezeList`. Only callable by genesis or change_epoch transactions.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(FreezeList { id: object::freeze_list() });
}

#[test_only]
public(package) fun create_for_testing(ctx: &TxContext) {
    create(ctx);
}
