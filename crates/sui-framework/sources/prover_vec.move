// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::prover_vec {
    // remove an element at index from a vector and return the resulting vector (redirects to a
    // function in vector theory)
    spec native fun remove<T>(v: vector<T>, elem_idx: u64): vector<T>;
}
