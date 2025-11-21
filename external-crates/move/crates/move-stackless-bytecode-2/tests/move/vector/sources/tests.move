// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module vec::tests {
    public fun create_empty(): vector<bool> {
        let empty: vector<bool> = vector[];
        empty
    }

    public fun create(x0: u8, x1: u8): vector<u8> {
        let v: vector<u8> = vector[x0, x1];
        v
    }

    public fun create_with_elements(): vector<u8> {
        // A vector of u8 elements.
        let v: vector<u8> = vector[10, 20, 30];
        v
    }

    public fun create_vector_of_vectors(): vector<vector<u8>> {
        // A vector of vector<u8> elements.
        let vv: vector<vector<u8>> = vector[
            vector[10, 20],
            vector[30, 40]
        ];
        vv
    }

    public fun vec_imm_borrow(v: &vector<u8>): &u8 {
        &v[1]
    }

    public fun vec_mut_borrow(v: &mut vector<u8>): &mut u8 {
        &mut v[1]
    }

    public fun vec_swap(v: &mut vector<u8>) {
        vector::swap(v, 0, 1);
    }

    public fun push_and_pop(): (vector<u8>, u8) {
        // A vector of u8 elements.
        let mut v: vector<u8> = vector[1, 2, 3];

        // Push an element to the end of the vector.
        vector::push_back(&mut v, 4);

        // Pop an element from the end of the vector.
        let last: u8 = vector::pop_back(&mut v);
        (v, last)
    }
}
