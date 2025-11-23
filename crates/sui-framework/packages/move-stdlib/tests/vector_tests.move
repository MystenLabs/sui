// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::vector_tests;

use std::unit_test::assert_eq;

public struct R has store {}
public struct Droppable has drop {}
public struct NotDroppable {}

#[test]
fun test_singleton_contains() {
    assert_eq!(vector[0u64][0], 0);
    assert_eq!(vector[true][0], true);
    assert_eq!(vector[@0x1][0], @0x1);
}

#[test]
fun test_singleton_len() {
    assert_eq!(vector[0u64].length(), 1);
    assert_eq!(vector[true].length(), 1);
    assert_eq!(vector[@0x1].length(), 1);
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
fun append_singletons() {
    let mut v1 = vector[0u64];
    let v2 = vector[1];
    v1.append(v2);
    assert_eq!(v1.length(), 2);
    assert_eq!(v1[0], 0);
    assert_eq!(v1[1], 1);
}

#[test]
fun append_respects_order_empty_lhs() {
    let mut v1 = vector[];
    let mut v2 = vector[];
    v2.push_back(0u64);
    v2.push_back(1);
    v2.push_back(2);
    v2.push_back(3);
    v1.append(v2);
    assert!(!v1.is_empty());
    assert_eq!(v1.length(), 4);
    assert_eq!(v1[0], 0);
    assert_eq!(v1[1], 1);
    assert_eq!(v1[2], 2);
    assert_eq!(v1[3], 3);
}

#[test]
fun append_respects_order_empty_rhs() {
    let mut v1 = vector[];
    let v2 = vector[];
    v1.push_back(0u64);
    v1.push_back(1);
    v1.push_back(2);
    v1.push_back(3);
    v1.append(v2);
    assert!(!v1.is_empty());
    assert_eq!(v1.length(), 4);
    assert_eq!(v1[0], 0);
    assert_eq!(v1[1], 1);
    assert_eq!(v1[2], 2);
    assert_eq!(v1[3], 3);
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
    assert_eq!(v1.length(), 8);
    let mut i = 0;
    while (i < 8) {
        assert_eq!(v1[i], i);
        i = i + 1;
    }
}

#[test, expected_failure(vector_error, minor_status = 1, location = Self)]
fun borrow_out_of_range() {
    let mut v = vector[];
    v.push_back(7u64);
    &v[1];
}

#[test]
fun vector_contains() {
    let mut vec = vector[];
    assert!(!vec.contains(&0u64));

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
    v.push_back(42u64);
    v.pop_back();
    v.destroy_empty();
}

#[test, expected_failure(vector_error, minor_status = 3, location = Self)]
fun destroy_non_empty() {
    let mut v = vector[];
    v.push_back(42u64);
    v.destroy_empty();
}

#[test]
fun get_set_work() {
    let mut vec = vector[];
    vec.push_back(0u64);
    vec.push_back(1);
    assert_eq!(vec[1], 1);
    assert_eq!(vec[0], 0);

    *&mut vec[0] = 17;
    assert_eq!(vec[1], 1);
    assert_eq!(vec[0], 17);
}

#[test, expected_failure(vector_error, minor_status = 2, location = Self)]
fun pop_out_of_range() {
    let mut v = vector<u64>[];
    v.pop_back();
}

#[test]
fun swap_different_indices() {
    let mut vec = vector[];
    vec.push_back(0u64);
    vec.push_back(1);
    vec.push_back(2);
    vec.push_back(3);
    vec.swap(0, 3);
    vec.swap(1, 2);
    assert_eq!(vec[0], 3);
    assert_eq!(vec[1], 2);
    assert_eq!(vec[2], 1);
    assert_eq!(vec[3], 0);
}

#[test]
fun swap_same_index() {
    let mut vec = vector[];
    vec.push_back(0u64);
    vec.push_back(1);
    vec.push_back(2);
    vec.push_back(3);
    vec.swap(1, 1);
    assert_eq!(vec[0], 0);
    assert_eq!(vec[1], 1);
    assert_eq!(vec[2], 2);
    assert_eq!(vec[3], 3);
}

#[test]
fun remove_singleton_vector() {
    let mut v = vector[];
    v.push_back(0u64);
    assert_eq!(v.remove(0), 0);
    assert_eq!(v.length(), 0);
}

#[test]
fun remove_nonsingleton_vector() {
    let mut v = vector[];
    v.push_back(0u64);
    v.push_back(1);
    v.push_back(2);
    v.push_back(3);

    assert_eq!(v.remove(1), 1);
    assert_eq!(v.length(), 3);
    assert_eq!(v[0], 0);
    assert_eq!(v[1], 2);
    assert_eq!(v[2], 3);
}

#[test]
fun remove_nonsingleton_vector_last_elem() {
    let mut v = vector[];
    v.push_back(0u64);
    v.push_back(1);
    v.push_back(2);
    v.push_back(3);

    assert_eq!(v.remove(3), 3);
    assert_eq!(v.length(), 3);
    assert_eq!(v[0], 0);
    assert_eq!(v[1], 1);
    assert_eq!(v[2], 2);
}

#[test, expected_failure(abort_code = vector::EINDEX_OUT_OF_BOUNDS)]
fun remove_empty_vector() {
    let mut v = vector<u64>[];
    v.remove(0);
}

#[test, expected_failure(abort_code = vector::EINDEX_OUT_OF_BOUNDS)]
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
    assert_eq!(is_empty, v.is_empty());
}

#[test]
fun reverse_singleton_vector() {
    let mut v = vector[];
    v.push_back(0u64);
    assert_eq!(v[0], 0);
    v.reverse();
    assert_eq!(v[0], 0);
}

#[test]
fun reverse_vector_nonempty_even_length() {
    let mut v = vector[];
    v.push_back(0u64);
    v.push_back(1);
    v.push_back(2);
    v.push_back(3);

    assert_eq!(v[0], 0);
    assert_eq!(v[1], 1);
    assert_eq!(v[2], 2);
    assert_eq!(v[3], 3);

    v.reverse();

    assert_eq!(v[3], 0);
    assert_eq!(v[2], 1);
    assert_eq!(v[1], 2);
    assert_eq!(v[0], 3);
}

#[test]
fun reverse_vector_nonempty_odd_length_non_singleton() {
    let mut v = vector[];
    v.push_back(0u64);
    v.push_back(1);
    v.push_back(2);

    assert_eq!(v[0], 0);
    assert_eq!(v[1], 1);
    assert_eq!(v[2], 2);

    v.reverse();

    assert_eq!(v[2], 0);
    assert_eq!(v[1], 1);
    assert_eq!(v[0], 2);
}

#[test, expected_failure(vector_error, minor_status = 1, location = Self)]
fun swap_empty() {
    let mut v = vector<u64>[];
    v.swap(0, 0);
}

#[test, expected_failure(vector_error, minor_status = 1, location = Self)]
fun swap_out_of_range() {
    let mut v = vector<u64>[];

    v.push_back(0);
    v.push_back(1);
    v.push_back(2);
    v.push_back(3);

    v.swap(1, 10);
}

#[test, expected_failure(abort_code = std::vector::EINDEX_OUT_OF_BOUNDS)]
fun swap_remove_empty() {
    let mut v = vector<u64>[];
    v.swap_remove(0);
}

#[test]
fun swap_remove_singleton() {
    let mut v = vector<u64>[];
    v.push_back(0);
    assert_eq!(v.swap_remove(0), 0);
    assert!(v.is_empty());
}

#[test]
fun swap_remove_inside_vector() {
    let mut v = vector[];
    v.push_back(0u64);
    v.push_back(1);
    v.push_back(2);
    v.push_back(3);

    assert_eq!(v[0], 0);
    assert_eq!(v[1], 1);
    assert_eq!(v[2], 2);
    assert_eq!(v[3], 3);

    assert_eq!(v.swap_remove(1), 1);
    assert_eq!(v.length(), 3);

    assert_eq!(v[0], 0);
    assert_eq!(v[1], 3);
    assert_eq!(v[2], 2);
}

#[test]
fun swap_remove_end_of_vector() {
    let mut v = vector[];
    v.push_back(0u64);
    v.push_back(1);
    v.push_back(2);
    v.push_back(3);

    assert_eq!(v[0], 0);
    assert_eq!(v[1], 1);
    assert_eq!(v[2], 2);
    assert_eq!(v[3], 3);

    assert_eq!(v.swap_remove(3), 3);
    assert_eq!(v.length(), 3);

    assert_eq!(v[0], 0);
    assert_eq!(v[1], 1);
    assert_eq!(v[2], 2);
}

#[test, expected_failure(vector_error, minor_status = 1, location = std::vector)]
fun swap_remove_out_of_range() {
    let mut v = vector[];
    v.push_back(0u64);
    v.swap_remove(1);
}

#[test]
fun skip() {
    assert_eq!(vector[0, 1, 2u64].skip(2), vector[2]);
    assert_eq!(vector[0, 1, 2u64].skip(0), vector[0, 1, 2]);
    assert_eq!(vector[0u64, 1, 2].skip(3), vector[]);
}

#[test]
fun take() {
    assert_eq!(vector[0, 1, 2u64].take(0), vector[]);
    assert_eq!(vector[0, 1, 2].take(1), vector[0u64]);
    assert_eq!(vector[0, 1, 2u64].take(2), vector[0, 1]);
    assert_eq!(vector[0, 1, 2].take(3), vector[0, 1u64, 2]);
}

#[test, expected_failure]
fun take_fail() {
    vector[0, 1u64, 2].take(4); // out of bounds (taking 4 elements)
}

#[test]
fun push_back_and_borrow() {
    let mut v = vector[];
    v.push_back(7u64);
    assert!(!v.is_empty());
    assert_eq!(v.length(), 1);
    assert_eq!(v[0], 7);

    v.push_back(8);
    assert_eq!(v.length(), 2);
    assert_eq!(v[0], 7);
    assert_eq!(v[1], 8);
}

#[test]
fun index_of_empty_not_has() {
    let v = vector[];
    let (has, index) = v.index_of(&true);
    assert!(!has);
    assert_eq!(index, 0);
}

#[test]
fun index_of_nonempty_not_has() {
    let mut v = vector[];
    v.push_back(false);
    let (has, index) = v.index_of(&true);
    assert!(!has);
    assert_eq!(index, 0);
}

#[test]
fun index_of_nonempty_has() {
    let mut v = vector[];
    v.push_back(false);
    v.push_back(true);
    let (has, index) = v.index_of(&true);
    assert!(has);
    assert_eq!(index, 1);
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
    assert_eq!(index, 1);
}

#[test]
fun length() {
    let mut empty = vector[];
    assert_eq!(empty.length(), 0);
    let mut i = 0;
    let max_len = 42;
    while (i < max_len) {
        empty.push_back(i);
        assert_eq!(empty.length(), i + 1);
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
        i = i + 1u64;
    };

    while (i > 0) {
        assert_eq!(v.pop_back(), i - 1);
        i = i - 1;
    };
}

#[test_only]
fun test_natives_with_type<T>(mut x1: T, mut x2: T): (T, T) {
    let mut v = vector[];
    assert_eq!(v.length(), 0);
    v.push_back(x1);
    assert_eq!(v.length(), 1);
    v.push_back(x2);
    assert_eq!(v.length(), 2);
    v.swap(0, 1);
    x1 = v.pop_back();
    assert_eq!(v.length(), 1);
    x2 = v.pop_back();
    assert_eq!(v.length(), 0);
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

    test_natives_with_type<Droppable>(Droppable {}, Droppable {});
    (NotDroppable {}, NotDroppable {}) =
        test_natives_with_type<NotDroppable>(
            NotDroppable {},
            NotDroppable {},
        );
}

#[test]
fun test_insert() {
    let mut v = vector[7];
    v.insert(6, 0);
    assert_eq!(v, vector[6, 7u64]);

    let mut v = vector[7, 9u64];
    v.insert(8, 1);
    assert_eq!(v, vector[7, 8, 9]);

    let mut v = vector[6, 7];
    v.insert(5, 0);
    assert_eq!(v, vector[5u64, 6, 7]);

    let mut v = vector[5, 6, 8];
    v.insert(7, 2);
    assert_eq!(v, vector[5, 6, 7, 8u64]);
}

#[test]
fun insert_at_end() {
    let mut v = vector[];
    v.insert(6u64, 0);
    assert_eq!(v, vector[6]);

    v.insert(7, 1);
    assert_eq!(v, vector[6, 7]);
}

#[test, expected_failure(abort_code = std::vector::EINDEX_OUT_OF_BOUNDS)]
fun insert_out_of_range() {
    let mut v = vector[7u64];
    v.insert(6, 2);
}

#[test]
fun size_limit_ok() {
    let mut v = vector[];
    let mut i = 0;
    // Limit is currently 1024 * 54
    let max_len = 1024 * 53u64;

    while (i < max_len) {
        v.push_back(i);
        i = i + 1;
    };
}

#[test, expected_failure(out_of_gas, location = Self)]
fun size_limit_fail() {
    let mut v = vector[];
    let mut i = 0;
    // Choose value beyond limit
    let max_len = 1024u64 * 1024;

    while (i < max_len) {
        v.push_back(i);
        i = i + 1;
    };
}

#[test]
fun test_string_aliases() {
    assert_eq!(b"hello_world".to_string().length(), 11);
    assert!(b"hello_world".try_to_string().is_some());

    assert_eq!(b"hello_world".to_ascii_string().length(), 11);
    assert!(b"hello_world".try_to_ascii_string().is_some());
}

// === Macros ===

#[test]
fun test_destroy_macro() {
    vector<u8>[].destroy!(|_| assert!(false)); // very funky

    let mut acc = 0;
    vector[10, 20, 30, 40u64].destroy!(|e| acc = acc + e);
    assert_eq!(acc, 100);

    vector[10, 20u64, 30, 40].destroy!(|e| e); // return value
    vector[10, 20, 30u64, 40].destroy!(|_| {}); // no return
}

#[test]
fun test_count_macro() {
    assert_eq!(vector<u8>[].count!(|e| *e == 2), 0);
    assert_eq!(vector[0, 1, 2, 3u64].count!(|e| *e == 2), 1);
    assert_eq!(vector[0, 1, 2, 3].count!(|e| *e % 2 == 0u64), vector[0u64, 2u64].length());
}

#[test]
fun test_tabulate_macro() {
    let v = vector::tabulate!(10, |i| i);
    assert_eq!(v, vector[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);

    let v = vector::tabulate!(5, |i| 10 - i);
    assert_eq!(v, vector[10, 9, 8, 7, 6]);

    let v = vector::tabulate!(0, |i| i);
    assert_eq!(v, vector<u64>[]);
}

#[test]
fun test_do_macro() {
    vector<u8>[].do!(|_| assert!(false)); // should never run
    vector<u8>[].do_ref!(|_| assert!(false));
    vector<u8>[].do_mut!(|_| assert!(false));

    let mut acc = 0;
    vector[10, 20, 30, 40].do!(|e| acc = acc + e);
    assert_eq!(acc, 100);

    let vec = vector[10, 20u64];
    vec.do!(|e| acc = acc + e);
    assert_eq!(vector[10, 20], vec);

    let mut acc = 0;
    vector[10, 20, 30, 40].do_ref!(|e| acc = acc + *e);
    assert_eq!(acc, 100u64);

    let mut vec = vector[10, 20, 30, 40];
    vec.do_mut!(|e| *e = *e + 1u64);
    assert_eq!(vec, vector[11, 21, 31, 41]);

    vector[10u64].do!(|e| e); // return value
    vector[10u64].do!(|_| {}); // no return

    vector[10u64].do_ref!(|e| *e); // return value
    vector[10u64].do_ref!(|_| {}); // no return

    vector[10u64].do_mut!(|e| *e); // return value
    vector[10u64].do_mut!(|_| {}); // no return
}

#[test]
fun test_map_macro() {
    let e = vector<u8>[];
    assert_eq!(e.map!(|e| e + 1), vector[]);

    let r = vector[0, 1, 2, 3];
    assert_eq!(r.map!(|e| e + 1), vector[1, 2, 3u64, 4]);

    let r = vector[0, 1, 2, 3];
    assert_eq!(r.map_ref!(|e| *e * 2u64), vector[0, 2, 4, 6]);
}

#[test]
fun filter_macro() {
    let e = vector<u8>[];
    assert_eq!(e.filter!(|e| *e % 2 == 0), vector[]);

    let r = vector[0, 1, 2, 3];
    assert_eq!(r.filter!(|e| *e % 2u64 == 0), vector[0, 2]);
}

#[test]
fun partition_macro() {
    let e = vector<u8>[];
    let (even, odd) = e.partition!(|e| (*e % 2) == 0);
    assert_eq!(even, vector[]);
    assert_eq!(odd, vector[]);

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
    assert_eq!(r.find_index!(|e| *e == 100).destroy_some(), 2);
    assert!(r.find_index!(|e| *e == 10_000u64).is_none());

    let v = vector[Droppable {}, Droppable {}];
    let idx = v.find_index!(|e| e == Droppable{});
    assert_eq!(idx.destroy_some(), 0);
    assert!(&v[idx.destroy_some()] == &Droppable{});
}

#[test]
fun find_indices_macro() {
    let e = vector<u8>[];
    assert_eq!(e.find_indices!(|e| *e == 0), vector[]);
    assert_eq!(e.find_indices!(|_| true), vector[]);

    let r = vector[0u64, 10, 100, 1_000];
    assert_eq!(r.find_indices!(|e| *e == 100), vector[2]);
    assert_eq!(r.find_indices!(|e| *e == 10_000), vector[]);
    assert_eq!(r.find_indices!(|e| *e / 10 > 0), vector[1, 2, 3]);
}

#[test]
fun fold_macro() {
    let e = vector<u8>[];
    assert!(e.fold!(0, |acc, e| acc + e) == 0);

    let r = vector[0, 1, 2, 3u64];
    assert!(r.fold!(10, |acc, e| acc + e) == 16);
}

#[test]
fun test_flatten() {
    assert!(vector<vector<u8>>[].flatten().is_empty());
    assert!(vector<vector<u8>>[vector[], vector[]].flatten().is_empty());
    assert!(vector[vector[1u64]].flatten() == vector[1]);
    assert!(vector[vector[1], vector[]].flatten() == vector[1u64]);
    assert!(vector[vector[1], vector[2u64]].flatten() == vector[1, 2]);
    assert!(vector[vector[1u64], vector[2, 3]].flatten() == vector[1, 2, 3]);
}

#[test]
fun any_all_macro() {
    assert!(vector<u8>[].any!(|e| *e == 2) == false);
    assert!(vector<u8>[].all!(|e| *e == 2) == true);
    assert!(vector[0u64, 1, 2, 3].any!(|e| *e == 2));
    assert!(!vector[0, 1, 2, 3u64].any!(|e| *e == 4));
    assert!(vector[0, 1u64, 2, 3].all!(|e| *e < 4));
    assert!(!vector[0, 1, 2u64, 3].all!(|e| *e < 3));
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

    vector[1].zip_do!(vector[2u64], |a, b| a + b); // return value
    vector[1u64].zip_do!(vector[2u64], |_, _| {}); // no return
}

#[test]
fun zip_do_undroppable_macro() {
    let v1 = vector[NotDroppable {}, NotDroppable {}];
    let v2 = vector[NotDroppable {}, NotDroppable {}];

    v1.zip_do!(v2, |a, b| {
        let NotDroppable {} = a;
        let NotDroppable {} = b;
    });
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

    vector[1].zip_do_reverse!(vector[2u64], |a, b| a + b); // return value
    vector[1u64].zip_do_reverse!(vector[2u64], |_, _| {}); // no return
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

    v1.zip_do_ref!(&v2, |a, b| *a + *b); // return value
    v1.zip_do_ref!(&v2, |_, _| {}); // no return
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

    v1.zip_do_mut!(&mut v2, |a, b| *a + *b); // return value
    v1.zip_do_mut!(&mut v2, |_, _| {}); // no return
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

// A helper struct for testing stability of sort macros
public struct Indexed has copy, drop {
    value: u64,
    index: u64,
}

const UNSORTED_100: vector<u8> =
    x"ed0c0f0ef96c4537d606ad4e1482e6369cf2db785363f16c2786bf866731cf072f030d29c5acac94e9aa10bd402fba01efa38d7c3f6399b3d8d8fc137bbcaa3e5b6db5b3dd163e041dea8c45dab677a9f49aa6ee25a55e52a5618aa0da08af2a4e8e7b1b";
const UNSORTED_50: vector<u8> =
    x"2f2420312a0f20050a19312028251731202b250c29301927040f06030220022d2314060c2a2e23021b11292a1d2e301a1c07";
const UNSORTED_40: vector<u8> =
    x"6deb6c0d0e1ca38d4a59ceb875dd36b857699bd34980ac6e79e7f7a8f999684b6ab929b070da47f7";
const UNSORTED_30: vector<u8> = x"0220021409192d182808091c20170e0e121e04290521181428151b2f150f";

#[test]
fun insertion_sort_by_macro() {
    let mut arr = UNSORTED_100;
    arr.insertion_sort_by!(|a, b| *a <= *b);
    assert!(arr.is_sorted_by!(|a, b| *a <= *b));

    let mut arr = UNSORTED_50;
    arr.insertion_sort_by!(|a, b| *a < *b);
    assert!(arr.is_sorted_by!(|a, b| *a <= *b));

    let mut arr = UNSORTED_40;
    arr.insertion_sort_by!(|a, b| *a < *b);
    assert!(arr.is_sorted_by!(|a, b| *a <= *b));

    let mut arr = UNSORTED_30;
    arr.insertion_sort_by!(|a, b| *a < *b);
    assert!(arr.is_sorted_by!(|a, b| *a <= *b));
}

#[test]
fun merge_sort_by_macro() {
    let mut arr = UNSORTED_100;
    arr.merge_sort_by!(|a, b| *a < *b);
    assert!(arr.is_sorted_by!(|a, b| *a <= *b));

    let mut arr = UNSORTED_50;
    arr.merge_sort_by!(|a, b| *a < *b);
    assert!(arr.is_sorted_by!(|a, b| *a <= *b));

    let mut arr = UNSORTED_40;
    arr.merge_sort_by!(|a, b| *a < *b);
    assert!(arr.is_sorted_by!(|a, b| *a <= *b));

    let mut arr = UNSORTED_30;
    arr.merge_sort_by!(|a, b| *a < *b);
    assert!(arr.is_sorted_by!(|a, b| *a <= *b));
}

#[random_test]
// this test may time out if we take large vectors
// so to optimize, we pop the vector to a smaller size
fun sort_by_random_set(mut v: vector<u8>) {
    let mut arr = vector::tabulate!(v.length().min(100), |_| v.pop_back());
    arr.insertion_sort_by!(|a, b| *a <= *b);
    assert!(arr.is_sorted_by!(|a, b| *a <= *b));
}

#[test]
fun test_insertion_sort_is_stable_sort_by() {
    let mut arr = vector[
        Indexed { value: 1, index: 0 },
        Indexed { value: 2, index: 1 },
        Indexed { value: 3, index: 2 },
        Indexed { value: 3, index: 3 },
        Indexed { value: 1, index: 4 },
        Indexed { value: 2, index: 5 },
    ];

    arr.insertion_sort_by!(|a, b| a.value <= b.value);
    assert_eq!(
        arr,
        vector[
            Indexed { value: 1, index: 0 },
            Indexed { value: 1, index: 4 },
            Indexed { value: 2, index: 1 },
            Indexed { value: 2, index: 5 },
            Indexed { value: 3, index: 2 },
            Indexed { value: 3, index: 3 },
        ],
    );

    // reverse the comparison function
    arr.insertion_sort_by!(|a, b| b.value <= a.value);
    assert_eq!(
        arr,
        vector[
            Indexed { value: 3, index: 2 },
            Indexed { value: 3, index: 3 },
            Indexed { value: 2, index: 1 },
            Indexed { value: 2, index: 5 },
            Indexed { value: 1, index: 0 },
            Indexed { value: 1, index: 4 },
        ],
    );
}

#[test]
fun test_merge_sort_is_stable_sort_by() {
    let mut arr = vector[
        Indexed { value: 1, index: 0 },
        Indexed { value: 2, index: 1 },
        Indexed { value: 3, index: 2 },
        Indexed { value: 3, index: 3 },
        Indexed { value: 1, index: 4 },
        Indexed { value: 2, index: 5 },
    ];

    arr.merge_sort_by!(|a, b| a.value <= b.value);
    assert_eq!(
        arr,
        vector[
            Indexed { value: 1, index: 0 },
            Indexed { value: 1, index: 4 },
            Indexed { value: 2, index: 1 },
            Indexed { value: 2, index: 5 },
            Indexed { value: 3, index: 2 },
            Indexed { value: 3, index: 3 },
        ],
    );

    arr.merge_sort_by!(|a, b| a.value >= b.value);
    assert_eq!(
        arr,
        vector[
            Indexed { value: 3, index: 2 },
            Indexed { value: 3, index: 3 },
            Indexed { value: 2, index: 1 },
            Indexed { value: 2, index: 5 },
            Indexed { value: 1, index: 0 },
            Indexed { value: 1, index: 4 },
        ],
    );
}

#[test, allow(implicit_const_copy)]
fun test_is_sorted_by() {
    assert!(vector<u8>[].is_sorted_by!(|a, b| *a <= *b));
    assert!(vector<u8>[].is_sorted_by!(|a, b| *a <= *b));
    assert!(vector<u8>[].is_sorted_by!(|_, _| false));
    assert!(vector[0u64].is_sorted_by!(|a, b| *a <= *b));
    assert!(vector[0u64].is_sorted_by!(|a, b| *a <= *b));
    assert!(vector[0u64].is_sorted_by!(|_, _| false));
    assert!(!vector[1, 2, 4, 3u64].is_sorted_by!(|a, b| *a < *b));

    assert!(!UNSORTED_30.is_sorted_by!(|a, b| *a <= *b));
    assert!(!UNSORTED_40.is_sorted_by!(|a, b| *a <= *b));
    assert!(!UNSORTED_50.is_sorted_by!(|a, b| *a <= *b));
    assert!(!UNSORTED_100.is_sorted_by!(|a, b| *a <= *b));
}

#[test]
fun take_while() {
    assert_eq!(vector[0, 1, 2u64].take_while!(|e| *e > 0), vector[]);
    assert_eq!(vector[0, 1, 2u64].take_while!(|e| *e < 2), vector[0, 1]);
    assert_eq!(vector[0, 1, 2u64].take_while!(|e| *e == 0), vector[0]);
    assert_eq!(vector[0, 1, 2].take_while!(|e| *e < 3), vector[0u64, 1, 2]);
}

#[test]
fun skip_while() {
    assert_eq!(vector[0, 1, 2].skip_while!(|e| *e > 0), vector[0u64, 1, 2]);
    assert_eq!(vector[0, 1, 2u64].skip_while!(|e| *e < 2), vector[2]);
    assert_eq!(vector[0, 1, 2u64].skip_while!(|e| *e == 0), vector[1, 2]);

    let v = vector[1, 1, 1, 2, 2, 2, 3, 3, 3];
    assert_eq!(v.skip_while!(|_| false), v);
    assert_eq!(v.skip_while!(|e| *e == 1u64), vector[2, 2, 2, 3, 3, 3]);
    assert_eq!(v.skip_while!(|e| *e <= 2), vector[3, 3, 3]);
    assert_eq!(v.skip_while!(|_| true), vector[]);
}
