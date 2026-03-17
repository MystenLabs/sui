// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Index syntax defined in the base module, used in a test extension.
module a::m {
    public struct S has drop { t: vector<u64> }

    public fun new(t: vector<u64>): S { S { t } }

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
    #[test]
    fun test_use_index_from_base() {
        let s = new(vector[5, 10, 15]);
        assert!(&s[0] == 5, 0);
        assert!(&s[2] == 15, 1);
    }

    #[test]
    fun test_mut_index_from_base() {
        let mut s = new(vector[5, 10, 15]);
        assert!(&mut s[0] == 5, 0);
    }
}
