// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_test_code::cool_stuff {

    struct CoolEvent has copy, drop {
        is_cool: bool,
    }

    fun init(_ctx: &sui::tx_context::TxContext) {
        let event = CoolEvent { is_cool: true };
        sui::event::emit(event);
    }
}
