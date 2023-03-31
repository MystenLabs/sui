// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module deepbook::sui {
    use std::option::none;

    use sui::coin::create_currency;
    use sui::transfer::{public_freeze_object, public_share_object};
    use sui::tx_context::TxContext;

    const DECIMAL: u8 = 8;

    struct SUI has drop {}

    fun init(witness: SUI, ctx: &mut TxContext) {
        let (treasury_cap, metadata) = create_currency<SUI>(witness, DECIMAL, b"SUI", b"SUI", b"SUI", none(), ctx);
        public_freeze_object(metadata);
        public_share_object(treasury_cap);
    }
}
