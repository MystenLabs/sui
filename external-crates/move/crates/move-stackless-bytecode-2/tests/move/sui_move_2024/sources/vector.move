// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_move_2024::vector_test {

    public fun create_empty(): vector<bool> {
        // An empty vector of bool elements.
        let empty: vector<bool> = vector[];
        empty
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

    public fun create_and_destroy_vector() {
        let mut v = vector[1, 2, 3];
        v.pop_back(); // Removes the last element
        v.pop_back(); // Removes the second last element
        v.pop_back(); // Removes the first element
        // At this point, the vector is empty.
        v.destroy_empty();
    }

    // public fun tabulate_vector(): vector<u64> {
    //     // A vector of u64 elements with values from 0 to 9.
    //     vector::tabulate!(10, |i| i )
    // }
}