// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An immutable object for stress testing reads of frozen objects.
module basics::immutable_object {
    public struct ImmutableData has key {
        id: UID,
        value: u64,
    }

    public fun create_and_freeze(value: u64, ctx: &mut TxContext) {
        transfer::freeze_object(ImmutableData {
            id: object::new(ctx),
            value,
        })
    }

    public fun value(data: &ImmutableData): u64 {
        data.value
    }
}
