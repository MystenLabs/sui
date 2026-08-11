// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module base_addr::friend_module;

public struct X has drop, store {
    v: bool,
}

public struct Y has drop, store {
    v: u64,
}

public struct Z has drop, store {
    x: X,
}

public fun make_x(v: bool): X {
    X { v }
}

public fun make_y(v: u64): Y {
    Y { v }
}

public fun make_z(v: bool): Z {
    let x = X { v };
    Z { x }
}
