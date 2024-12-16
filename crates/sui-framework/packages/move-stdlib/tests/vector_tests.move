// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::vector_tests;

public struct R has store {}
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
fun append_singletons() {
    let mut v1 = vector[0];
    let v2 = vector[1];
    v1.append(v2);
    assert!(v1.length() == 2);
    assert!(v1[0] == 0);
    assert!(v1[1] == 1);
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

    let v = vector[Droppable {}, Droppable {}];
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
