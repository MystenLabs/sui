// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module z::a {
    public fun bar(x: u64): u64 {
        z::b::foo(sui::math::max(x, 42))
    }
}
