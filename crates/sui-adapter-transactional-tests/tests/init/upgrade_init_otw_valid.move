// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests the that the OTW can be successfully used, e.g. in creating a currency

//# init --addresses v0=0x0 v1=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::m {
}

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
module v1::m {
}
module v1::has_otw {
    public struct HAS_OTW has drop {
    }
    fun init(otw: HAS_OTW, ctx: &mut TxContext) {
        let (cap, metadata) = sui::coin::create_currency(
            otw,
            2,
            b"has_otw",
            b"has_otw",
            b"has_otw",
            option::none(),
            ctx,
        );
        transfer::public_freeze_object(cap);
        transfer::public_freeze_object(metadata);
    }
}

//# view-object 2,1

//# view-object 2,2
