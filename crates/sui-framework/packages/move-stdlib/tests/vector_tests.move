// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::vector_tests {
    use std::vector;

    public struct R has store { }
    public struct Droppable has drop {}
    public struct NotDroppable {}

    #[test]
    fun test_singleton_contains() {
        assert!(vector[0][0] == 0, 0);
        assert!(vector[true][0] == true, 0);
        assert!(vector[@0x1][0] == @0x1, 0);
    }

    #[test]
    fun test_singleton_len() {
        assert!(&vector[0].length() == 1, 0);
        assert!(&vector[true].length() == 1, 0);
        assert!(&vector[@0x1].length() == 1, 0);
    }

    #[test]
    fun test_empty_is_empty() {
        assert!(vector<u64>[].is_empty(), 0);
    }

    #[test]
    fun append_empties_is_empty() {
        let mut v1 = vector<u64>[];
        let v2 = vector<u64>[];
        v1.append(v2);
        assert!(v1.is_empty(), 0);
    }

    #[test]
    fun append_respects_order_empty_lhs() {
        let mut v1 = vector[];
        let mut v2 = vector[];
        v2.push_back(0);
        v2.push_back(1);
        v2.push_back(2);
        v2.push_back(3);
        v1.append(v2);
        assert!(!v1.is_empty(), 0);
        assert!(v1.length() == 4, 1);
        assert!(v1[0] == 0, 2);
        assert!(v1[1] == 1, 3);
        assert!(v1[2] == 2, 4);
        assert!(v1[3] == 3, 5);
    }

    #[test]
    fun append_respects_order_empty_rhs() {
        let mut v1 = vector[];
        let v2 = vector[];
        v1.push_back(0);
        v1.push_back(1);
        v1.push_back(2);
        v1.push_back(3);
        v1.append(v2);
        assert!(!v1.is_empty(), 0);
        assert!(v1.length() == 4, 1);
        assert!(v1[0] == 0, 2);
        assert!(v1[1] == 1, 3);
        assert!(v1[2] == 2, 4);
        assert!(v1[3] == 3, 5);
    }

    #[test]
    fun append_respects_order_nonempty_rhs_lhs() {
        let mut v1 = vector[];
        let mut v2 = vector[];
        v1.push_back(0);
        v1.push_back(1);
        v1.push_back(2);
        v1.push_back(3);
        v2.push_back(4);
        v2.push_back(5);
        v2.push_back(6);
        v2.push_back(7);
        v1.append(v2);
        assert!(!v1.is_empty(), 0);
        assert!(v1.length() == 8, 1);
        let mut i = 0;
        while (i < 8) {
            assert!(v1[i] == i, i);
            i = i + 1;
        }
    }

    #[test]
    #[expected_failure(vector_error, minor_status = 1, location = Self)]
    fun borrow_out_of_range() {
        let mut v = vector[];
        v.push_back(7);
        &v[1];
    }

    #[test]
    fun vector_contains() {
        let mut vec = vector[];
        assert!(!vec.contains(&0), 1);

        vec.push_back(0);
        assert!(vec.contains(&0), 2);
        assert!(!vec.contains(&1), 3);

        vec.push_back(1);
        assert!(vec.contains(&0), 4);
        assert!(vec.contains(&1), 5);
        assert!(!vec.contains(&2), 6);

        vec.push_back(2);
        assert!(vec.contains(&0), 7);
        assert!(vec.contains(&1), 8);
        assert!(vec.contains(&2), 9);
        assert!(!vec.contains(&3), 10);
    }

    #[test]
    fun destroy_empty() {
        vector<u64>[].destroy_empty();
        vector<R>[].destroy_empty();
        vector::empty<u64>().destroy_empty();
        vector::empty<R>().destroy_empty();
    }

    #[test]
    fun destroy_empty_with_pops() {
        let mut v = vector[];
        v.push_back(42);
        v.pop_back();
        v.destroy_empty();
    }

    #[test]
    #[expected_failure(vector_error, minor_status = 3, location = Self)]
    fun destroy_non_empty() {
        let mut v = vector[];
        v.push_back(42);
        v.destroy_empty();
    }

    #[test]
    fun get_set_work() {
        let mut vec = vector[];
        vec.push_back(0);
        vec.push_back(1);
        assert!(vec[1] == 1, 0);
        assert!(vec[0] == 0, 1);

        *&mut vec[0] = 17;
        assert!(vec[1] == 1, 0);
        assert!(vec[0] == 17, 0);
    }

    #[test]
    #[expected_failure(vector_error, minor_status = 2, location = Self)]
    fun pop_out_of_range() {
        let mut v = vector<u64>[];
        v.pop_back();
    }

    #[test]
    fun swap_different_indices() {
        let mut vec = vector[];
        vec.push_back(0);
        vec.push_back(1);
        vec.push_back(2);
        vec.push_back(3);
        vec.swap(0, 3);
        vec.swap(1, 2);
        assert!(vec[0] == 3, 0);
        assert!(vec[1] == 2, 0);
        assert!(vec[2] == 1, 0);
        assert!(vec[3] == 0, 0);
    }

    #[test]
    fun swap_same_index() {
        let mut vec = vector[];
        vec.push_back(0);
        vec.push_back(1);
        vec.push_back(2);
        vec.push_back(3);
        vec.swap(1, 1);
        assert!(vec[0] == 0, 0);
        assert!(vec[1] == 1, 0);
        assert!(vec[2] == 2, 0);
        assert!(vec[3] == 3, 0);
    }

    #[test]
    fun remove_singleton_vector() {
        let mut v = vector[];
        v.push_back(0);
        assert!(v.remove(0) == 0, 0);
        assert!(v.length() == 0, 0);
    }

    #[test]
    fun remove_nonsingleton_vector() {
        let mut v = vector[];
        v.push_back(0);
        v.push_back(1);
        v.push_back(2);
        v.push_back(3);

        assert!(v.remove(1) == 1, 0);
        assert!(v.length() == 3, 0);
        assert!(v[0] == 0, 0);
        assert!(v[1] == 2, 0);
        assert!(v[2] == 3, 0);
    }

    #[test]
    fun remove_nonsingleton_vector_last_elem() {
        let mut v = vector[];
        v.push_back(0);
        v.push_back(1);
        v.push_back(2);
        v.push_back(3);

        assert!(v.remove(3) == 3, 0);
        assert!(v.length() == 3, 0);
        assert!(v[0] == 0, 0);
        assert!(v[1] == 1, 0);
        assert!(v[2] == 2, 0);
    }

    #[test]
    #[expected_failure(abort_code = vector::EINDEX_OUT_OF_BOUNDS)]
    fun remove_empty_vector() {
        let mut v = vector<u64>[];
        v.remove(0);
    }

    #[test]
    #[expected_failure(abort_code = vector::EINDEX_OUT_OF_BOUNDS)]
    fun remove_out_of_bound_index() {
        let mut v = vector<u64>[];
        v.push_back(0);
        v.remove(1);
    }

    #[test]
    fun reverse_vector_empty() {
        let mut v = vector<u64>[];
        let is_empty = v.is_empty();
        v.reverse();
        assert!(is_empty == v.is_empty(), 0);
    }

    #[test]
    fun reverse_singleton_vector() {
        let mut v = vector[];
        v.push_back(0);
        assert!(v[0] == 0, 1);
        v.reverse();
        assert!(v[0] == 0, 2);
    }

    #[test]
    fun reverse_vector_nonempty_even_length() {
        let mut v = vector[];
        v.push_back(0);
        v.push_back(1);
        v.push_back(2);
        v.push_back(3);

        assert!(v[0] == 0, 1);
        assert!(v[1] == 1, 2);
        assert!(v[2] == 2, 3);
        assert!(v[3] == 3, 4);

        v.reverse();

        assert!(v[3] == 0, 5);
        assert!(v[2] == 1, 6);
        assert!(v[1] == 2, 7);
        assert!(v[0] == 3, 8);
    }

    #[test]
    fun reverse_vector_nonempty_odd_length_non_singleton() {
        let mut v = vector[];
        v.push_back(0);
        v.push_back(1);
        v.push_back(2);

        assert!(v[0] == 0, 1);
        assert!(v[1] == 1, 2);
        assert!(v[2] == 2, 3);

        v.reverse();

        assert!(v[2] == 0, 4);
        assert!(v[1] == 1, 5);
        assert!(v[0] == 2, 6);
    }

    #[test]
    #[expected_failure(vector_error, minor_status = 1, location = Self)]
    fun swap_empty() {
        let mut v = vector<u64>[];
        v.swap(0, 0);
    }

    #[test]
    #[expected_failure(vector_error, minor_status = 1, location = Self)]
    fun swap_out_of_range() {
        let mut v = vector<u64>[];

        v.push_back(0);
        v.push_back(1);
        v.push_back(2);
        v.push_back(3);

        v.swap(1, 10);
    }

    #[test]
    #[expected_failure(abort_code = std::vector::EINDEX_OUT_OF_BOUNDS)]
    fun swap_remove_empty() {
        let mut v = vector<u64>[];
        v.swap_remove(0);
    }

    #[test]
    fun swap_remove_singleton() {
        let mut v = vector<u64>[];
        v.push_back(0);
        assert!(v.swap_remove(0) == 0, 0);
        assert!(v.is_empty(), 1);
    }

    #[test]
    fun swap_remove_inside_vector() {
        let mut v = vector[];
        v.push_back(0);
        v.push_back(1);
        v.push_back(2);
        v.push_back(3);

        assert!(v[0] == 0, 1);
        assert!(v[1] == 1, 2);
        assert!(v[2] == 2, 3);
        assert!(v[3] == 3, 4);

        assert!(v.swap_remove(1) == 1, 5);
        assert!(v.length() == 3, 6);

        assert!(v[0] == 0, 7);
        assert!(v[1] == 3, 8);
        assert!(v[2] == 2, 9);

    }

    #[test]
    fun swap_remove_end_of_vector() {
        let mut v = vector[];
        v.push_back(0);
        v.push_back(1);
        v.push_back(2);
        v.push_back(3);

        assert!(v[0] == 0, 1);
        assert!(v[1] == 1, 2);
        assert!(v[2] == 2, 3);
        assert!(v[3] == 3, 4);

        assert!(v.swap_remove(3) == 3, 5);
        assert!(v.length() == 3, 6);

        assert!(v[0] == 0, 7);
        assert!(v[1] == 1, 8);
        assert!(v[2] == 2, 9);
    }

    #[test]
    #[expected_failure(vector_error, minor_status = 1, location = std::vector)]
    fun swap_remove_out_of_range() {
        let mut v = vector[];
        v.push_back(0);
        v.swap_remove(1);
    }

    #[test]
    fun push_back_and_borrow() {
        let mut v = vector[];
        v.push_back(7);
        assert!(!v.is_empty(), 0);
        assert!(v.length() == 1, 1);
        assert!(v[0] == 7, 2);

        v.push_back(8);
        assert!(v.length() == 2, 3);
        assert!(v[0] == 7, 4);
        assert!(v[1] == 8, 5);
    }

    #[test]
    fun index_of_empty_not_has() {
        let v = vector[];
        let (has, index) = v.index_of(&true);
        assert!(!has, 0);
        assert!(index == 0, 1);
    }

    #[test]
    fun index_of_nonempty_not_has() {
        let mut v = vector[];
        v.push_back(false);
        let (has, index) = v.index_of(&true);
        assert!(!has, 0);
        assert!(index == 0, 1);
    }

    #[test]
    fun index_of_nonempty_has() {
        let mut v = vector[];
        v.push_back(false);
        v.push_back(true);
        let (has, index) = v.index_of(&true);
        assert!(has, 0);
        assert!(index == 1, 1);
    }

    // index_of will return the index first occurence that is equal
    #[test]
    fun index_of_nonempty_has_multiple_occurences() {
        let mut v = vector[];
        v.push_back(false);
        v.push_back(true);
        v.push_back(true);
        let (has, index) = v.index_of(&true);
        assert!(has, 0);
        assert!(index == 1, 1);
    }

    #[test]
    fun length() {
        let mut empty = vector[];
        assert!(empty.length() == 0, 0);
        let mut i = 0;
        let max_len = 42;
        while (i < max_len) {
            empty.push_back(i);
            assert!(empty.length() == i + 1, i);
            i = i + 1;
        }
    }

    #[test]
    fun pop_push_back() {
        let mut v = vector[];
        let mut i = 0;
        let max_len = 42;

        while (i < max_len) {
            v.push_back(i);
            i = i + 1;
        };

        while (i > 0) {
            assert!(v.pop_back() == i - 1, i);
            i = i - 1;
        };
    }

    #[test_only]
    fun test_natives_with_type<T>(mut x1: T, mut x2: T): (T, T) {
        let mut v = vector[];
        assert!(v.length() == 0, 0);
        v.push_back(x1);
        assert!(v.length() == 1, 1);
        v.push_back(x2);
        assert!(v.length() == 2, 2);
        v.swap(0, 1);
        x1 = v.pop_back();
        assert!(v.length() == 1, 3);
        x2 = v.pop_back();
        assert!(v.length() == 0, 4);
        v.destroy_empty();
        (x1, x2)
    }

    #[test]
    fun test_natives_with_different_instantiations() {
        test_natives_with_type<u8>(1u8, 2u8);
        test_natives_with_type<u16>(45356u16, 25345u16);
        test_natives_with_type<u32>(45356u32, 28768867u32);
        test_natives_with_type<u64>(1u64, 2u64);
        test_natives_with_type<u128>(1u128, 2u128);
        test_natives_with_type<u256>(45356u256, 253458768867u256);
        test_natives_with_type<bool>(true, false);
        test_natives_with_type<address>(@0x1, @0x2);

        test_natives_with_type<vector<u8>>(vector[], vector[]);

        test_natives_with_type<Droppable>(Droppable{}, Droppable{});
        (NotDroppable {}, NotDroppable {}) = test_natives_with_type<NotDroppable>(
            NotDroppable {},
            NotDroppable {}
        );
    }

    #[test]
    fun test_insert() {
        let mut v = vector[7];
        v.insert(6, 0);
        assert!(v == vector[6, 7], 0);

        let mut v = vector[7, 9];
        v.insert(8, 1);
        assert!(v == vector[7, 8, 9], 0);

        let mut v = vector[6, 7];
        v.insert(5, 0);
        assert!(v == vector[5, 6, 7], 0);

        let mut v = vector[5, 6, 8];
        v.insert(7, 2);
        assert!(v == vector[5, 6, 7, 8], 0);
    }

    #[test]
    fun insert_at_end() {
        let mut v = vector[];
        v.insert(6, 0);
        assert!(v == vector[6], 0);

        v.insert(7, 1);
        assert!(v == vector[6, 7], 0);
    }

    #[test]
    #[expected_failure(abort_code = std::vector::EINDEX_OUT_OF_BOUNDS)]
    fun insert_out_of_range() {
        let mut v = vector[7];
        v.insert(6, 2);
    }

    #[test]
    fun size_limit_ok() {
        let mut v = vector[];
        let mut i = 0;
        // Limit is currently 1024 * 54
        let max_len = 1024 * 53;

        while (i < max_len) {
            v.push_back(i);
            i = i + 1;
        };
    }

    #[test]
    #[expected_failure(out_of_gas, location = Self)]
    fun size_limit_fail() {
        let mut v = vector[];
        let mut i = 0;
        // Choose value beyond limit
        let max_len = 1024 * 1024;

        while (i < max_len) {
            v.push_back(i);
            i = i + 1;
        };
    }

    #[test]
    fun test_string_aliases() {
        assert!(b"hello_world".to_string().length() == 11, 0);
        assert!(b"hello_world".try_to_string().is_some(), 1);

        assert!(b"hello_world".to_ascii_string().length() == 11, 2);
        assert!(b"hello_world".try_to_ascii_string().is_some(), 3);
    }
}
