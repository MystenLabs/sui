// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Vector-related utilities.
module big_vector::utils {
    /// Pop elements from the back of `v` until its length equals `n`,
    /// returning the elements that were popped in the order they
    /// appeared in `v`.
    public(package) fun pop_until<T>(v: &mut vector<T>, n: u64): vector<T> {
        let mut res = vector[];
        while (v.length() > n) {
            res.push_back(v.pop_back());
        };

        res.reverse();
        res
    }

    /// Pop `n` elements from the back of `v`, returning the elements
    /// that were popped in the order they appeared in `v`.
    ///
    /// Aborts if `v` has fewer than `n` elements.
    public(package) fun pop_n<T>(v: &mut vector<T>, mut n: u64): vector<T> {
        let mut res = vector[];
        while (n > 0) {
            res.push_back(v.pop_back());
            n = n - 1;
        };

        res.reverse();
        res
    }
}
