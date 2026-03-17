// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module a::m {
    public struct S has drop { t: vector<u64> }

    public fun new(t: vector<u64>): S { S { t } }
}

#[test_only]
extend module a::m {
    #[syntax(index)]
    fun borrow(s: &S, i: u64): &u64 {
        &s.t[i]
    }

    #[syntax(index)]
    fun borrow_mut(s: &mut S, i: u64): &mut u64 {
        &mut s.t[i]
    }

    #[test]
    fun test_index_read() {
        let s = new(vector[10, 20, 30]);
        assert!(&s[0] == 10, 0);
        assert!(&s[1] == 20, 1);
        assert!(&s[2] == 30, 2);
    }

    #[test]
    fun test_index_mut_read() {
        let mut s = new(vector[10, 20, 30]);
        assert!(&mut s[1] == 20, 0);
    }
}
