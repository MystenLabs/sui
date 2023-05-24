// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0

//# publish --lint
module test::unused_types {
    use sui::tx_context::TxContext;

    struct UNUSED_TYPES has drop {}

    struct UnusedType has drop {}

    fun init(_otw: UNUSED_TYPES, _ctx: &mut TxContext) {
        // should not label OTW as unused even though it is never packed
    }

    public fun use_type(): UsedType {
        // we define pack = use
        UsedType {}
    }

    // make sure that defining type after use does not matter
    struct UsedType has drop {}

    // doesn't count as used
    public fun use_no_pack(x: UnusedType): UnusedType {
        x
    }
}
