// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module base_addr::friend_module {

    public struct X has store, drop {
        v: bool,
    }

    public struct Y has store, drop {
        v: u64,
    }

    public fun make_x(v: bool): X {
        X { v }
    }

    public fun make_y(v: u64): Y {
        Y { v }
    }
}
