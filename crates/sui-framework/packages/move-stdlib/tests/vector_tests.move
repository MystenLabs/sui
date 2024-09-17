// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::vector_tests {
    public struct R has store { }
    public struct Droppable has drop {}
    public struct NotDroppable {}

    #[test]
    fun test_singleton_contains() {
        assert!(vector[0][0] == 0);
        assert!(vector[true][0] == true);
        assert!(vector[@0x1][0] == @0x1);
    }

    #[test]
    fun test_singleton_len() {
        assert!(&vector[0].length() == 1);
        assert!(&vector[true].length() == 1);
        assert!(&vector[@0x1].length() == 1);
    }

    #[test]
    fun test_empty_is_empty() {
        assert!(vector<u64>[].is_empty());
    }

    #[test]
    fun append_empties_is_empty() {
        let mut v1 = vector<u64>[];
        let v2 = vector<u64>[];
        v1.append(v2);
        assert!(v1.is_empty());
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
        assert!(!v1.is_empty());
        assert!(v1.length() == 4);
        assert!(v1[0] == 0);
        assert!(v1[1] == 1);
        assert!(v1[2] == 2);
        assert!(v1[3] == 3);
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
        assert!(!v1.is_empty());
        assert!(v1.length() == 4);
        assert!(v1[0] == 0);
        assert!(v1[1] == 1);
        assert!(v1[2] == 2);
        assert!(v1[3] == 3);
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
        assert!(!v1.is_empty());
        assert!(v1.length() == 8);
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
        assert!(!vec.contains(&0));

        vec.push_back(0);
        assert!(vec.contains(&0));
        assert!(!vec.contains(&1));

        vec.push_back(1);
        assert!(vec.contains(&0));
        assert!(vec.contains(&1));
        assert!(!vec.contains(&2));

        vec.push_back(2);
        assert!(vec.contains(&0));
        assert!(vec.contains(&1));
        assert!(vec.contains(&2));
        assert!(!vec.contains(&3));
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
        assert!(vec[1] == 1);
        assert!(vec[0] == 0);

        *&mut vec[0] = 17;
        assert!(vec[1] == 1);
        assert!(vec[0] == 17);
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
        assert!(vec[0] == 3);
        assert!(vec[1] == 2);
        assert!(vec[2] == 1);
        assert!(vec[3] == 0);
    }

    #[test]
    fun swap_same_index() {
        let mut vec = vector[];
        vec.push_back(0);
        vec.push_back(1);
        vec.push_back(2);
        vec.push_back(3);
        vec.swap(1, 1);
        assert!(vec[0] == 0);
        assert!(vec[1] == 1);
        assert!(vec[2] == 2);
        assert!(vec[3] == 3);
    }

    #[test]
    fun remove_singleton_vector() {
        let mut v = vector[];
        v.push_back(0);
        assert!(v.remove(0) == 0);
        assert!(v.length() == 0);
    }

    #[test]
    fun remove_nonsingleton_vector() {
        let mut v = vector[];
        v.push_back(0);
        v.push_back(1);
        v.push_back(2);
        v.push_back(3);

        assert!(v.remove(1) == 1);
        assert!(v.length() == 3);
        assert!(v[0] == 0);
        assert!(v[1] == 2);
        assert!(v[2] == 3);
    }

    #[test]
    fun remove_nonsingleton_vector_last_elem() {
        let mut v = vector[];
        v.push_back(0);
        v.push_back(1);
        v.push_back(2);
        v.push_back(3);

        assert!(v.remove(3) == 3);
        assert!(v.length() == 3);
        assert!(v[0] == 0);
        assert!(v[1] == 1);
        assert!(v[2] == 2);
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
        assert!(is_empty == v.is_empty());
    }

    #[test]
    fun reverse_singleton_vector() {
        let mut v = vector[];
        v.push_back(0);
        assert!(v[0] == 0);
        v.reverse();
        assert!(v[0] == 0);
    }

    #[test]
    fun reverse_vector_nonempty_even_length() {
        let mut v = vector[];
        v.push_back(0);
        v.push_back(1);
        v.push_back(2);
        v.push_back(3);

        assert!(v[0] == 0);
        assert!(v[1] == 1);
        assert!(v[2] == 2);
        assert!(v[3] == 3);

        v.reverse();

        assert!(v[3] == 0);
        assert!(v[2] == 1);
        assert!(v[1] == 2);
        assert!(v[0] == 3);
    }

    #[test]
    fun reverse_vector_nonempty_odd_length_non_singleton() {
        let mut v = vector[];
        v.push_back(0);
        v.push_back(1);
        v.push_back(2);

        assert!(v[0] == 0);
        assert!(v[1] == 1);
        assert!(v[2] == 2);

        v.reverse();

        assert!(v[2] == 0);
        assert!(v[1] == 1);
        assert!(v[0] == 2);
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
        assert!(v.swap_remove(0) == 0);
        assert!(v.is_empty());
    }

    #[test]
    fun swap_remove_inside_vector() {
        let mut v = vector[];
        v.push_back(0);
        v.push_back(1);
        v.push_back(2);
        v.push_back(3);

        assert!(v[0] == 0);
        assert!(v[1] == 1);
        assert!(v[2] == 2);
        assert!(v[3] == 3);

        assert!(v.swap_remove(1) == 1);
        assert!(v.length() == 3);

        assert!(v[0] == 0);
        assert!(v[1] == 3);
        assert!(v[2] == 2);

    }

    #[test]
    fun swap_remove_end_of_vector() {
        let mut v = vector[];
        v.push_back(0);
        v.push_back(1);
        v.push_back(2);
        v.push_back(3);

        assert!(v[0] == 0);
        assert!(v[1] == 1);
        assert!(v[2] == 2);
        assert!(v[3] == 3);

        assert!(v.swap_remove(3) == 3);
        assert!(v.length() == 3);

        assert!(v[0] == 0);
        assert!(v[1] == 1);
        assert!(v[2] == 2);
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
        assert!(!v.is_empty());
        assert!(v.length() == 1);
        assert!(v[0] == 7);

        v.push_back(8);
        assert!(v.length() == 2);
        assert!(v[0] == 7);
        assert!(v[1] == 8);
    }

    #[test]
    fun index_of_empty_not_has() {
        let v = vector[];
        let (has, index) = v.index_of(&true);
        assert!(!has);
        assert!(index == 0);
    }

    #[test]
    fun index_of_nonempty_not_has() {
        let mut v = vector[];
        v.push_back(false);
        let (has, index) = v.index_of(&true);
        assert!(!has);
        assert!(index == 0);
    }

    #[test]
    fun index_of_nonempty_has() {
        let mut v = vector[];
        v.push_back(false);
        v.push_back(true);
        let (has, index) = v.index_of(&true);
        assert!(has);
        assert!(index == 1);
    }

    // index_of will return the index first occurence that is equal
    #[test]
    fun index_of_nonempty_has_multiple_occurences() {
        let mut v = vector[];
        v.push_back(false);
        v.push_back(true);
        v.push_back(true);
        let (has, index) = v.index_of(&true);
        assert!(has);
        assert!(index == 1);
    }

    #[test]
    fun length() {
        let mut empty = vector[];
        assert!(empty.length() == 0);
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
        assert!(v.length() == 0);
        v.push_back(x1);
        assert!(v.length() == 1);
        v.push_back(x2);
        assert!(v.length() == 2);
        v.swap(0, 1);
        x1 = v.pop_back();
        assert!(v.length() == 1);
        x2 = v.pop_back();
        assert!(v.length() == 0);
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
        assert!(v == vector[6, 7]);

        let mut v = vector[7, 9];
        v.insert(8, 1);
        assert!(v == vector[7, 8, 9]);

        let mut v = vector[6, 7];
        v.insert(5, 0);
        assert!(v == vector[5, 6, 7]);

        let mut v = vector[5, 6, 8];
        v.insert(7, 2);
        assert!(v == vector[5, 6, 7, 8]);
    }

    #[test]
    fun insert_at_end() {
        let mut v = vector[];
        v.insert(6, 0);
        assert!(v == vector[6]);

        v.insert(7, 1);
        assert!(v == vector[6, 7]);
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
        assert!(b"hello_world".to_string().length() == 11);
        assert!(b"hello_world".try_to_string().is_some());

        assert!(b"hello_world".to_ascii_string().length() == 11);
        assert!(b"hello_world".try_to_ascii_string().is_some());
    }

    // === Macros ===

    #[test]
    fun test_destroy_macro() {
        vector<u8>[].destroy!(|_| assert!(false)); // very funky

        let mut acc = 0;
        vector[10, 20, 30, 40].destroy!(|e| acc = acc + e);
        assert!(acc == 100);
    }

    #[test]
    fun test_count_macro() {
        assert!(vector<u8>[].count!(|e| *e == 2) == 0);
        assert!(vector[0, 1, 2, 3].count!(|e| *e == 2) == 1);
        assert!(vector[0, 1, 2, 3].count!(|e| *e % 2 == 0) == vector[0, 2].length());
    }

    #[test]
    fun test_tabulate_macro() {
        let v = vector::tabulate!(10, |i| i);
        assert!(v == vector[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);

        let v = vector::tabulate!(5, |i| 10 - i);
        assert!(v == vector[10, 9, 8, 7, 6]);

        let v = vector::tabulate!(0, |i| i);
        assert!(v == vector<u64>[]);
    }

    #[test]
    fun test_do_macro() {
        vector<u8>[].do!(|_| assert!(false)); // should never run
        vector<u8>[].do_ref!(|_| assert!(false));
        vector<u8>[].do_mut!(|_| assert!(false));

        let mut acc = 0;
        vector[10, 20, 30, 40].do!(|e| acc = acc + e);
        assert!(acc == 100);

        let vec = vector[10, 20];
        vec.do!(|e| acc = acc + e);
        assert!(vector[10, 20] == vec);

        let mut acc = 0;
        vector[10, 20, 30, 40].do_ref!(|e| acc = acc + *e);
        assert!(acc == 100);

        let mut vec = vector[10, 20, 30, 40];
        vec.do_mut!(|e| *e = *e + 1);
        assert!(vec == vector[11, 21, 31, 41]);
    }

    #[test]
    fun test_map_macro() {
        let e = vector<u8>[];
        assert!(e.map!(|e| e + 1) == vector[]);

        let r = vector[0, 1, 2, 3];
        assert!(r.map!(|e| e + 1) == vector[1, 2, 3, 4]);

        let r = vector[0, 1, 2, 3];
        assert!(r.map_ref!(|e| *e * 2) == vector[0, 2, 4, 6]);
    }

    #[test]
    fun filter_macro() {
        let e = vector<u8>[];
        assert!(e.filter!(|e| *e % 2 == 0) == vector[]);

        let r = vector[0, 1, 2, 3];
        assert!(r.filter!(|e| *e % 2 == 0) == vector[0, 2]);
    }

    #[test]
    fun partition_macro() {
        let e = vector<u8>[];
        let (even, odd) = e.partition!(|e| (*e % 2) == 0);
        assert!(even == vector[]);
        assert!(odd == vector[]);

        let r = vector<u64>[0, 1, 2, 3];
        let (even, odd) = r.partition!(|e| (*e % 2) == 0);
        assert!(even == vector[0, 2]);
        assert!(odd == vector[1, 3]);
    }

    #[test]
    fun find_index_macro() {
        let e = vector<u8>[];
        assert!(e.find_index!(|e| *e == 0).is_none());
        assert!(e.find_index!(|_| true).is_none());

        let r = vector[0, 10, 100, 1_000];
        assert!(r.find_index!(|e| *e == 100).destroy_some() == 2);
        assert!(r.find_index!(|e| *e == 10_000).is_none());

        let v = vector[Droppable{}, Droppable{}];
        let idx = v.find_index!(|e| e == Droppable{});
        assert!(idx.destroy_some() == 0);
        assert!(&v[idx.destroy_some()] == Droppable{});
    }

    #[test]
    fun fold_macro() {
        let e = vector<u8>[];
        assert!(e.fold!(0, |acc, e| acc + e) == 0);

        let r = vector[0, 1, 2, 3];
        assert!(r.fold!(10, |acc, e| acc + e) == 16);
    }

    #[test]
    fun test_flatten() {
        assert!(vector<vector<u8>>[].flatten().is_empty());
        assert!(vector<vector<u8>>[vector[], vector[]].flatten().is_empty());
        assert!(vector[vector[1]].flatten() == vector[1]);
        assert!(vector[vector[1], vector[]].flatten() == vector[1]);
        assert!(vector[vector[1], vector[2]].flatten() == vector[1, 2]);
        assert!(vector[vector[1], vector[2, 3]].flatten() == vector[1, 2, 3]);
    }

    #[test]
    fun any_all_macro() {
        assert!(vector<u8>[].any!(|e| *e == 2) == false);
        assert!(vector<u8>[].all!(|e| *e == 2) == true);
        assert!(vector[0, 1, 2, 3].any!(|e| *e == 2));
        assert!(!vector[0, 1, 2, 3].any!(|e| *e == 4));
        assert!(vector[0, 1, 2, 3].all!(|e| *e < 4));
        assert!(!vector[0, 1, 2, 3].all!(|e| *e < 3));
    }

    #[test, expected_failure]
    fun zip_do_macro_fail() {
        let v1 = vector[1u64];
        let v2 = vector[4u64, 5];
        let mut res = vector[];
        v1.zip_do!(v2, |a, b| res.push_back(a + b));
    }

    #[test]
    fun zip_do_macro() {
        let v1 = vector[1u64, 2, 3];
        let v2 = vector[4u64, 5, 6];
        let mut res = vector[];
        v1.zip_do!(v2, |a, b| res.push_back(a + b));
        assert!(res == vector[5, 7, 9]);
    }

    #[test, expected_failure]
    fun zip_do_reverse_macro_fail() {
        let v1 = vector[1u64];
        let v2 = vector[4u64, 5];
        let mut res = vector[];
        v2.zip_do_reverse!(v1, |a, b| res.push_back(a + b));
    }

    #[test]
    fun zip_do_reverse_macro() {
        let v1 = vector[1u64, 2, 3];
        let v2 = vector[4u64, 5, 6];
        let mut res = vector[];
        v2.zip_do_reverse!(v1, |a, b| res.push_back(a + b));
        assert!(res == vector[9, 7, 5]);
    }

    #[test, expected_failure]
    fun zip_do_ref_macro_fail() {
        let v1 = vector[1u64];
        let v2 = vector[4u64, 5];
        let mut res = vector[];
        v2.zip_do_ref!(&v1, |a, b| res.push_back(*a + *b));
    }

    #[test]
    fun zip_do_ref_macro() {
        let v1 = vector[1u64, 2, 3];
        let v2 = vector[4u64, 5, 6];
        let mut res = vector[];
        v1.zip_do_ref!(&v2, |a, b| res.push_back(*a + *b));
        assert!(res == vector[5, 7, 9]);
    }

    #[test, expected_failure]
    fun zip_do_mut_macro_fail() {
        let mut v1 = vector[1u64];
        let mut v2 = vector[4u64, 5];
        v1.zip_do_mut!(&mut v2, |a, b| {
            let c = *a;
            *a = *b;
            *b = c;
        });
    }

    #[test]
    fun zip_do_mut_macro() {
        let mut v1 = vector[1u64, 2, 3];
        let mut v2 = vector[4u64, 5, 6];
        v1.zip_do_mut!(&mut v2, |a, b| {
            let c = *a;
            *a = *b;
            *b = c;
        });
        assert!(v1 == vector[4, 5, 6]);
        assert!(v2 == vector[1, 2, 3]);
    }

    #[test]
    fun zip_map_macro() {
        let v1 = vector[1u64, 2, 3];
        let v2 = vector[4u64, 5, 6];
        assert!(v1.zip_map!(v2, |a, b| a + b) == vector[5, 7, 9]);
    }

    #[test]
    fun zip_map_ref_macro() {
        let v1 = vector[1u64, 2, 3];
        let v2 = vector[4u64, 5, 6];
        assert!(v2.zip_map_ref!(&v1, |a, b| *a + *b) == vector[5, 7, 9]);
    }


    fun all_permutations<T: copy + drop>(mut data: vector<T>): vector<vector<T>> {
        let mut result = vector[];
        generate_permutations(&mut data, 0, &mut result);
        result
    }

    fun generate_permutations<T: copy>(
        data: &mut vector<T>,
        start: u64,
        result: &mut vector<vector<T>>,
    ) {
        if (start == data.length()) {
            result.push_back(*data);
            return
        };

        start.range_do!(data.length(), |i| {
            data.swap(start, i);
            generate_permutations(data, start + 1, result);
            data.swap(start, i);
        });
    }

    public struct Indexed has copy, drop {
        value: u64,
        index: u64,
    }

    const UNSORTED_1000: vector<u8> = x"4f3409142c3206064f575414535e1f0d1a371c622042221343545e153e075e1437375f31515f0f2357632227124548380e00052d40272607502e2901071240524a5854432c51243428192f2e3a4215462e4d2b62326146014f121714472e553e5d3f46363f5b3d47552b413e4e551702454a02075d1f32504c455849334759362e1c3d055e37430536155f3f4d2f3e391e3f38231960353f4650350c390a61545c145a6261525d61330f4c49135e620b45325f05294c5b1c1f115d4261261849341e5b1e5d3005533b10141f1f3a2831291b4d0825373b411e30005a5e5c613a15584e53103c0c3f1528303d1c3a57542c145d16085b53365d12505151511e453c025c632d23061d1c0660534657303c162842631c545b2404390a27352f41152b3d1e29380c080963186335461f270f32083045633d2b0f122407203f0c103b1307012d02392859515e1a22372531115932411625185d4d45461d10145e3c4d59311423141e2f57131204013551415c29513839473e2f2923031c40494d1e58030744484506295b16375f185b021013561e3d503d1e23223321295f0120345508583c1059231715480c07554e173c243e3702255f3e353f2e2a11012925383e2e0c5f424416581f44320634290a2b3f4f4b0d6301322e4d05253e180731360c5321424d58552c302a3b581402473712595920213752512e022f0f513b2518264d1d445b5f2029382a174614172d3156594e45234e5051033344092d07354557141e454a5a4038461d492d383506215f12553d46051805594b11093d3d224b441d28092616500e0724355f3d325105562c5a0d424d4427051742376049425a4149573c54222e2b034d51393a3b111824262a4354365e5416340e5b4534591c58072b5608271f2b070e62511f00062006145314370146230038402228320d37164f3a31325c633625391c232d5e59210b464f58614839075547613e58151a5f182e4b0517625f21305e433c16434f3f5b55504b615f02484e53394d345f17184001064b5d2b48475f635f565c17576232375c2f5f2b2258321263054952284a3d605917201d0b15343e0a46300e490b41591705001b532b375523491c4612243e1e0b0c524021173843114d4f3f1a543a1b1a0124305b2222614b392a30611036141a23075a5a54174b4e5c4a630c4f3127375320033133591e1e104f274e3544552a5c4134145c3f004336221847346023445f2b3357620c180f5b47283c0e0a4d1b4702550d3c21484031213d192b382205091f00201a3a4222355b5d1d63001c085120541e55173608160a592c14313c0f1203595f5758303a1f100b3e3a0a0f5f4a60505f1912054c301c5d1a410e02580a30036134505531005a4512171a0c5d6347153f0858";
    const UNSORTED_100: vector<u8> = x"ed0c0f0ef96c4537d606ad4e1482e6369cf2db785363f16c2786bf866731cf072f030d29c5acac94e9aa10bd402fba01efa38d7c3f6399b3d8d8fc137bbcaa3e5b6db5b3dd163e041dea8c45dab677a9f49aa6ee25a55e52a5618aa0da08af2a4e8e7b1b";
    const UNSORTED_50: vector<u8> = x"2f2420312a0f20050a19312028251731202b250c29301927040f06030220022d2314060c2a2e23021b11292a1d2e301a1c07";
    const UNSORTED_40: vector<u8> = x"6deb6c0d0e1ca38d4a59ceb875dd36b857699bd34980ac6e79e7f7a8f999684b6ab929b070da47f7";
    const UNSORTED_30: vector<u8> = x"0220021409192d182808091c20170e0e121e04290521181428151b2f150f";

    #[test]
    fun profile_snippets() {
        let _unsorted = UNSORTED_1000;
    }

    #[test]
    fun profile_30_insertion_sort_by() {
        let mut unsorted = UNSORTED_30;
        unsorted.insertion_sort_by!(|a, b| *a < *b);
        unsorted.reverse();
        unsorted.insertion_sort_by!(|a, b| *a < *b);
    }

    #[test]
    fun profile_30_merge_sort_by() {
        let mut unsorted = UNSORTED_30;
        unsorted.merge_sort_by!(|a, b| *a < *b);
        unsorted.reverse();
        unsorted.merge_sort_by!(|a, b| *a < *b);
    }

    #[test]
    fun profile_40_insertion_sort_by() {
        let mut unsorted = UNSORTED_40;
        unsorted.insertion_sort_by!(|a, b| *a < *b);
        unsorted.reverse();
        unsorted.insertion_sort_by!(|a, b| *a < *b);
    }

    #[test]
    fun profile_40_merge_sort_by() {
        let mut unsorted = UNSORTED_40;
        unsorted.merge_sort_by!(|a, b| *a < *b);
        unsorted.reverse();
        unsorted.merge_sort_by!(|a, b| *a < *b);
    }

    #[test]
    fun profile_50_insertion_sort_by() {
        let mut unsorted = UNSORTED_50;
        unsorted.insertion_sort_by!(|a, b| *a < *b);
        unsorted.reverse();
        unsorted.insertion_sort_by!(|a, b| *a < *b);
    }

    #[test]
    fun profile_50_merge_sort_by() {
        let mut unsorted = UNSORTED_50;
        unsorted.merge_sort_by!(|a, b| *a < *b);
        unsorted.reverse();
        unsorted.merge_sort_by!(|a, b| *a < *b);
    }

    #[test]
    fun profile_100_insertion_sort_by() {
        let mut unsorted = UNSORTED_100;
        unsorted.insertion_sort_by!(|a, b| *a < *b);
        unsorted.reverse();
        unsorted.insertion_sort_by!(|a, b| *a < *b);
    }

    #[test]
    fun profile_100_merge_sort_by() {
        let mut unsorted = UNSORTED_100;
        unsorted.merge_sort_by!(|a, b| *a < *b);
        unsorted.reverse();
        unsorted.merge_sort_by!(|a, b| *a < *b);
    }

    #[test]
    fun profile_1000_insertion_sort_by() {
        let mut unsorted = UNSORTED_1000;
        unsorted.insertion_sort_by!(|a, b| *a < *b);
        unsorted.reverse();
        unsorted.insertion_sort_by!(|a, b| *a < *b);
    }

    #[test]
    fun profile_1000_merge_sort_by() {
        let mut unsorted = UNSORTED_1000;
        unsorted.merge_sort_by!(|a, b| *a < *b);
        unsorted.reverse();
        unsorted.merge_sort_by!(|a, b| *a < *b);
    }

    #[test]
    fun test_merge_sort_by() {
        let data = vector[1, 2, 2, 3, 3, 3];
        let n = data.length();
        let vs = all_permutations(data);
        let vs = vs.map!(|v| {
            let mut i = 0;
            v.map!(|value| {
                let indexed = Indexed { value, index: i };
                i = i + 1;
                indexed
            })
        });
        vs.do!(|mut v| {
            v.merge_sort_by!(|a, b| a.value <= b.value);
            // is permutation
            n.do!(|i| assert!(v[i].value == data[i]));
            let mut counts = vector::tabulate!(n, |_| 0);
            v.do_ref!(|a| *&mut counts[a.index] = counts[a.index] + 1);
            counts.do!(|c| assert!(c == 1));
            // is sorted
            assert!(v.is_sorted_by!(|a, b| a.value <= b.value));
            // stable
            n.do!(|i| {
                (i + 1).range_do!(n, |j| {
                    assert!(v[i].value != v[j].value || v[i].index < v[j].index)
                })
            });
        });
    }

    #[test]
    fun test_is_sorted_by() {
        assert!(vector<u8>[].is_sorted_by!(|a, b| *a <= *b));
        assert!(vector<u8>[].is_sorted_by!(|a, b| *a > *b));
        assert!(vector<u8>[].is_sorted_by!(|_, _| false));
        assert!(vector[0].is_sorted_by!(|a, b| *a <= *b));
        assert!(vector[0].is_sorted_by!(|a, b| *a > *b));
        assert!(vector[0].is_sorted_by!(|_, _| false));
        assert!(!vector[1, 2, 4, 3].is_sorted_by!(|a, b| *a <= *b));

        let data = vector[1, 2, 2, 3, 3, 3];
        let vs = all_permutations(data);
        vs.do!(|v| {
            assert!(v == data || !v.is_sorted_by!(|a, b| *a <= *b));
            assert!(!v.is_sorted_by!(|_, _| false));
            assert!(v.is_sorted_by!(|_, _| true));
        });
    }
}
