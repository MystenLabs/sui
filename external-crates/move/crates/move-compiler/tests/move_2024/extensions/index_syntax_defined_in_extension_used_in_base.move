// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Base module uses index syntax on its own type, but the index syntax is only
// defined in a test extension. This should fail in normal mode (no index syntax
// defined) but succeed in test mode.
module a::m {
    public struct S has drop { t: vector<u64> }

    public fun new(t: vector<u64>): S { S { t } }

    public fun first(s: &S): &u64 {
        &s[0]
    }
}

#[test_only]
extend module a::m {
    #[syntax(index)]
    public fun borrow(s: &S, i: u64): &u64 {
        &s.t[i]
    }

    #[syntax(index)]
    public fun borrow_mut(s: &mut S, i: u64): &mut u64 {
        &mut s.t[i]
    }
}
