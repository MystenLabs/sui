// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Extension tries to redefine index syntax already defined in the base module.
module a::m {
    public struct S has drop { t: vector<u64> }

    #[syntax(index)]
    public fun borrow(s: &S, i: u64): &u64 {
        &s.t[i]
    }

    #[syntax(index)]
    public fun borrow_mut(s: &mut S, i: u64): &mut u64 {
        &mut s.t[i]
    }
}

#[test_only]
extend module a::m {
    #[syntax(index)]
    fun borrow2(s: &S, i: u64): &u64 {
        &s.t[i]
    }

    #[syntax(index)]
    fun borrow_mut2(s: &mut S, i: u64): &mut u64 {
        &mut s.t[i]
    }
}
