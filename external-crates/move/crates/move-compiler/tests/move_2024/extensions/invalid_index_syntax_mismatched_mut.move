// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Extension defines index syntax with wrong return type mutability.
module a::m {
    public struct S has drop { t: vector<u64> }
}

#[test_only]
extend module a::m {
    #[syntax(index)]
    fun borrow(s: &S, i: u64): &mut u64 {
        &mut s.t[i]
    }
}
