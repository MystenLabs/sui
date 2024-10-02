#[test_only]
module A::vector_tests {
    use std::vector as V;

    public struct R has store { }
    public struct Droppable has drop {}
    public struct NotDroppable {}

    #[test]
    fun test_singleton_contains() {
        assert!(*V::borrow(&V::singleton(0), 0) == 0, 0);
        assert!(*V::borrow(&V::singleton(true), 0) == true, 0);
        assert!(*V::borrow(&V::singleton(@0x1), 0) == @0x1, 0);
    }

    #[test]
    fun test_singleton_len() {
        assert!(V::length(&V::singleton(0)) == 1, 0);
        assert!(V::length(&V::singleton(true)) == 1, 0);
        assert!(V::length(&V::singleton(@0x1)) == 1, 0);
    }

    #[test]
    fun test_empty_is_empty() {
        assert!(V::is_empty(&V::empty<u64>()), 0);
    }

    #[test]
    fun append_empties_is_empty() {
        let mut v1 = V::empty<u64>();
        let v2 = V::empty<u64>();
        V::append(&mut v1, v2);
        assert!(V::is_empty(&v1), 0);
    }

    #[test]
    fun append_respects_order_empty_lhs() {
        let mut v1 = V::empty();
        let mut v2 = V::empty();
        V::push_back(&mut v2, 0);
        V::push_back(&mut v2, 1);
        V::push_back(&mut v2, 2);
        V::push_back(&mut v2, 3);
        V::append(&mut v1, v2);
        assert!(!V::is_empty(&v1), 0);
        assert!(V::length(&v1) == 4, 1);
        assert!(*V::borrow(&v1, 0) == 0, 2);
        assert!(*V::borrow(&v1, 1) == 1, 3);
        assert!(*V::borrow(&v1, 2) == 2, 4);
        assert!(*V::borrow(&v1, 3) == 3, 5);
    }

    #[test]
    fun append_respects_order_empty_rhs() {
        let mut v1 = V::empty();
        let v2 = V::empty();
        V::push_back(&mut v1, 0);
        V::push_back(&mut v1, 1);
        V::push_back(&mut v1, 2);
        V::push_back(&mut v1, 3);
        V::append(&mut v1, v2);
        assert!(!V::is_empty(&v1), 0);
        assert!(V::length(&v1) == 4, 1);
        assert!(*V::borrow(&v1, 0) == 0, 2);
        assert!(*V::borrow(&v1, 1) == 1, 3);
        assert!(*V::borrow(&v1, 2) == 2, 4);
        assert!(*V::borrow(&v1, 3) == 3, 5);
    }

    #[test]
    fun append_respects_order_nonempty_rhs_lhs() {
        let mut v1 = V::empty();
        let mut v2 = V::empty();
        V::push_back(&mut v1, 0);
        V::push_back(&mut v1, 1);
        V::push_back(&mut v1, 2);
        V::push_back(&mut v1, 3);
        V::push_back(&mut v2, 4);
        V::push_back(&mut v2, 5);
        V::push_back(&mut v2, 6);
        V::push_back(&mut v2, 7);
        V::append(&mut v1, v2);
        assert!(!V::is_empty(&v1), 0);
        assert!(V::length(&v1) == 8, 1);
        let mut i = 0;
        while (i < 8) {
            assert!(*V::borrow(&v1, i) == i, i);
            i = i + 1;
        }
    }

    #[test]
    #[expected_failure(vector_error, minor_status = 1, location = Self)]
    fun borrow_out_of_range() {
        let mut v = V::empty();
        V::push_back(&mut v, 7);
        V::borrow(&v, 1);
    }

    #[test]
    fun vector_contains() {
        let mut vec = V::empty();
        assert!(!V::contains(&vec, &0), 1);

        V::push_back(&mut vec, 0);
        assert!(V::contains(&vec, &0), 2);
        assert!(!V::contains(&vec, &1), 3);

        V::push_back(&mut vec, 1);
        assert!(V::contains(&vec, &0), 4);
        assert!(V::contains(&vec, &1), 5);
        assert!(!V::contains(&vec, &2), 6);

        V::push_back(&mut vec, 2);
        assert!(V::contains(&vec, &0), 7);
        assert!(V::contains(&vec, &1), 8);
        assert!(V::contains(&vec, &2), 9);
        assert!(!V::contains(&vec, &3), 10);
    }

    #[test]
    fun destroy_empty() {
        V::destroy_empty(V::empty<u64>());
        V::destroy_empty(V::empty<R>());
    }

    #[test]
    fun destroy_empty_with_pops() {
        let mut v = V::empty();
        V::push_back(&mut v, 42);
        V::pop_back(&mut v);
        V::destroy_empty(v);
    }

    #[test]
    #[expected_failure(vector_error, minor_status = 3, location = Self)]
    fun destroy_non_empty() {
        let mut v = V::empty();
        V::push_back(&mut v, 42);
        V::destroy_empty(v);
    }

    #[test]
    fun get_set_work() {
        let mut vec = V::empty();
        V::push_back(&mut vec, 0);
        V::push_back(&mut vec, 1);
        assert!(*V::borrow(&vec, 1) == 1, 0);
        assert!(*V::borrow(&vec, 0) == 0, 1);

        *V::borrow_mut(&mut vec, 0) = 17;
        assert!(*V::borrow(&vec, 1) == 1, 0);
        assert!(*V::borrow(&vec, 0) == 17, 0);
    }

    #[test]
    #[expected_failure(vector_error, minor_status = 2, location = Self)]
    fun pop_out_of_range() {
        let mut v = V::empty<u64>();
        V::pop_back(&mut v);
    }

    #[test]
    fun swap_different_indices() {
        let mut vec = V::empty();
        V::push_back(&mut vec, 0);
        V::push_back(&mut vec, 1);
        V::push_back(&mut vec, 2);
        V::push_back(&mut vec, 3);
        V::swap(&mut vec, 0, 3);
        V::swap(&mut vec, 1, 2);
        assert!(*V::borrow(&vec, 0) == 3, 0);
        assert!(*V::borrow(&vec, 1) == 2, 0);
        assert!(*V::borrow(&vec, 2) == 1, 0);
        assert!(*V::borrow(&vec, 3) == 0, 0);
    }

    #[test]
    fun swap_same_index() {
        let mut vec = V::empty();
        V::push_back(&mut vec, 0);
        V::push_back(&mut vec, 1);
        V::push_back(&mut vec, 2);
        V::push_back(&mut vec, 3);
        V::swap(&mut vec, 1, 1);
        assert!(*V::borrow(&vec, 0) == 0, 0);
        assert!(*V::borrow(&vec, 1) == 1, 0);
        assert!(*V::borrow(&vec, 2) == 2, 0);
        assert!(*V::borrow(&vec, 3) == 3, 0);
    }

    #[test]
    fun remove_singleton_vector() {
        let mut v = V::empty();
        V::push_back(&mut v, 0);
        assert!(V::remove(&mut v, 0) == 0, 0);
        assert!(V::length(&v) == 0, 0);
    }

    #[test]
    fun remove_nonsingleton_vector() {
        let mut v = V::empty();
        V::push_back(&mut v, 0);
        V::push_back(&mut v, 1);
        V::push_back(&mut v, 2);
        V::push_back(&mut v, 3);

        assert!(V::remove(&mut v, 1) == 1, 0);
        assert!(V::length(&v) == 3, 0);
        assert!(*V::borrow(&v, 0) == 0, 0);
        assert!(*V::borrow(&v, 1) == 2, 0);
        assert!(*V::borrow(&v, 2) == 3, 0);
    }

    #[test]
    fun remove_nonsingleton_vector_last_elem() {
        let mut v = V::empty();
        V::push_back(&mut v, 0);
        V::push_back(&mut v, 1);
        V::push_back(&mut v, 2);
        V::push_back(&mut v, 3);

        assert!(V::remove(&mut v, 3) == 3, 0);
        assert!(V::length(&v) == 3, 0);
        assert!(*V::borrow(&v, 0) == 0, 0);
        assert!(*V::borrow(&v, 1) == 1, 0);
        assert!(*V::borrow(&v, 2) == 2, 0);
    }

    #[test]
    #[expected_failure(abort_code = V::EINDEX_OUT_OF_BOUNDS)]
    fun remove_empty_vector() {
        let mut v = V::empty<u64>();
        V::remove(&mut v, 0);
    }

    #[test]
    #[expected_failure(abort_code = V::EINDEX_OUT_OF_BOUNDS)]
    fun remove_out_of_bound_index() {
        let mut v = V::empty<u64>();
        V::push_back(&mut v, 0);
        V::remove(&mut v, 1);
    }

    #[test]
    fun reverse_vector_empty() {
        let mut v = V::empty<u64>();
        let is_empty = V::is_empty(&v);
        V::reverse(&mut v);
        assert!(is_empty == V::is_empty(&v), 0);
    }

    #[test]
    fun reverse_singleton_vector() {
        let mut v = V::empty();
        V::push_back(&mut v, 0);
        assert!(*V::borrow(&v, 0) == 0, 1);
        V::reverse(&mut v);
        assert!(*V::borrow(&v, 0) == 0, 2);
    }

    #[test]
    fun reverse_vector_nonempty_even_length() {
        let mut v = V::empty();
        V::push_back(&mut v, 0);
        V::push_back(&mut v, 1);
        V::push_back(&mut v, 2);
        V::push_back(&mut v, 3);

        assert!(*V::borrow(&v, 0) == 0, 1);
        assert!(*V::borrow(&v, 1) == 1, 2);
        assert!(*V::borrow(&v, 2) == 2, 3);
        assert!(*V::borrow(&v, 3) == 3, 4);

        V::reverse(&mut v);

        assert!(*V::borrow(&v, 3) == 0, 5);
        assert!(*V::borrow(&v, 2) == 1, 6);
        assert!(*V::borrow(&v, 1) == 2, 7);
        assert!(*V::borrow(&v, 0) == 3, 8);
    }

    #[test]
    fun reverse_vector_nonempty_odd_length_non_singleton() {
        let mut v = V::empty();
        V::push_back(&mut v, 0);
        V::push_back(&mut v, 1);
        V::push_back(&mut v, 2);

        assert!(*V::borrow(&v, 0) == 0, 1);
        assert!(*V::borrow(&v, 1) == 1, 2);
        assert!(*V::borrow(&v, 2) == 2, 3);

        V::reverse(&mut v);

        assert!(*V::borrow(&v, 2) == 0, 4);
        assert!(*V::borrow(&v, 1) == 1, 5);
        assert!(*V::borrow(&v, 0) == 2, 6);
    }

    #[test]
    #[expected_failure(vector_error, minor_status = 1, location = Self)]
    fun swap_empty() {
        let mut v = V::empty<u64>();
        V::swap(&mut v, 0, 0);
    }

    #[test]
    #[expected_failure(vector_error, minor_status = 1, location = Self)]
    fun swap_out_of_range() {
        let mut v = V::empty<u64>();

        V::push_back(&mut v, 0);
        V::push_back(&mut v, 1);
        V::push_back(&mut v, 2);
        V::push_back(&mut v, 3);

        V::swap(&mut v, 1, 10);
    }

    #[test]
    #[expected_failure(abort_code = V::EINDEX_OUT_OF_BOUNDS)]
    fun swap_remove_empty() {
        let mut v = V::empty<u64>();
        V::swap_remove(&mut v, 0);
    }

    #[test]
    fun swap_remove_singleton() {
        let mut v = V::empty<u64>();
        V::push_back(&mut v, 0);
        assert!(V::swap_remove(&mut v, 0) == 0, 0);
        assert!(V::is_empty(&v), 1);
    }

    #[test]
    fun swap_remove_inside_vector() {
        let mut v = V::empty();
        V::push_back(&mut v, 0);
        V::push_back(&mut v, 1);
        V::push_back(&mut v, 2);
        V::push_back(&mut v, 3);

        assert!(*V::borrow(&v, 0) == 0, 1);
        assert!(*V::borrow(&v, 1) == 1, 2);
        assert!(*V::borrow(&v, 2) == 2, 3);
        assert!(*V::borrow(&v, 3) == 3, 4);

        assert!(V::swap_remove(&mut v, 1) == 1, 5);
        assert!(V::length(&v) == 3, 6);

        assert!(*V::borrow(&v, 0) == 0, 7);
        assert!(*V::borrow(&v, 1) == 3, 8);
        assert!(*V::borrow(&v, 2) == 2, 9);

    }

    #[test]
    fun swap_remove_end_of_vector() {
        let mut v = V::empty();
        V::push_back(&mut v, 0);
        V::push_back(&mut v, 1);
        V::push_back(&mut v, 2);
        V::push_back(&mut v, 3);

        assert!(*V::borrow(&v, 0) == 0, 1);
        assert!(*V::borrow(&v, 1) == 1, 2);
        assert!(*V::borrow(&v, 2) == 2, 3);
        assert!(*V::borrow(&v, 3) == 3, 4);

        assert!(V::swap_remove(&mut v, 3) == 3, 5);
        assert!(V::length(&v) == 3, 6);

        assert!(*V::borrow(&v, 0) == 0, 7);
        assert!(*V::borrow(&v, 1) == 1, 8);
        assert!(*V::borrow(&v, 2) == 2, 9);
    }

    #[test]
    #[expected_failure(vector_error, minor_status = 1, location = std::vector)]
    fun swap_remove_out_of_range() {
        let mut v = V::empty();
        V::push_back(&mut v, 0);
        V::swap_remove(&mut v, 1);
    }

    #[test]
    fun push_back_and_borrow() {
        let mut v = V::empty();
        V::push_back(&mut v, 7);
        assert!(!V::is_empty(&v), 0);
        assert!(V::length(&v) == 1, 1);
        assert!(*V::borrow(&v, 0) == 7, 2);

        V::push_back(&mut v, 8);
        assert!(V::length(&v) == 2, 3);
        assert!(*V::borrow(&v, 0) == 7, 4);
        assert!(*V::borrow(&v, 1) == 8, 5);
    }

    #[test]
    fun index_of_empty_not_has() {
        let v = V::empty();
        let (has, index) = V::index_of(&v, &true);
        assert!(!has, 0);
        assert!(index == 0, 1);
    }

    #[test]
    fun index_of_nonempty_not_has() {
        let mut v = V::empty();
        V::push_back(&mut v, false);
        let (has, index) = V::index_of(&v, &true);
        assert!(!has, 0);
        assert!(index == 0, 1);
    }

    #[test]
    fun index_of_nonempty_has() {
        let mut v = V::empty();
        V::push_back(&mut v, false);
        V::push_back(&mut v, true);
        let (has, index) = V::index_of(&v, &true);
        assert!(has, 0);
        assert!(index == 1, 1);
    }

    // index_of will return the index first occurence that is equal
    #[test]
    fun index_of_nonempty_has_multiple_occurences() {
        let mut v = V::empty();
        V::push_back(&mut v, false);
        V::push_back(&mut v, true);
        V::push_back(&mut v, true);
        let (has, index) = V::index_of(&v, &true);
        assert!(has, 0);
        assert!(index == 1, 1);
    }

    #[test]
    fun length() {
        let mut empty = V::empty();
        assert!(V::length(&empty) == 0, 0);
        let mut i = 0;
        let max_len = 42;
        while (i < max_len) {
            V::push_back(&mut empty, i);
            assert!(V::length(&empty) == i + 1, i);
            i = i + 1;
        }
    }

    #[test]
    fun pop_push_back() {
        let mut v = V::empty();
        let mut i = 0;
        let max_len = 42;

        while (i < max_len) {
            V::push_back(&mut v, i);
            i = i + 1;
        };

        while (i > 0) {
            assert!(V::pop_back(&mut v) == i - 1, i);
            i = i - 1;
        };
    }

    #[test_only]
    fun test_natives_with_type<T>(mut x1: T, mut x2: T): (T, T) {
        let mut v = V::empty();
        assert!(V::length(&v) == 0, 0);
        V::push_back(&mut v, x1);
        assert!(V::length(&v) == 1, 1);
        V::push_back(&mut v, x2);
        assert!(V::length(&v) == 2, 2);
        V::swap(&mut v, 0, 1);
        x1 = V::pop_back(&mut v);
        assert!(V::length(&v) == 1, 3);
        x2 = V::pop_back(&mut v);
        assert!(V::length(&v) == 0, 4);
        V::destroy_empty(v);
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

        test_natives_with_type<vector<u8>>(V::empty(), V::empty());

        test_natives_with_type<Droppable>(Droppable{}, Droppable{});
        (NotDroppable {}, NotDroppable {}) = test_natives_with_type<NotDroppable>(
            NotDroppable {},
            NotDroppable {}
        );
    }

    #[test]
    fun test_insert() {
        let mut v = vector[7];
        V::insert(&mut v, 6, 0);
        assert!(v == vector[6, 7], 0);

        let mut v = vector[7, 9];
        V::insert(&mut v, 8, 1);
        assert!(v == vector[7, 8, 9], 0);

        let mut v = vector[6, 7];
        V::insert(&mut v, 5, 0);
        assert!(v == vector[5, 6, 7], 0);

        let mut v = vector[5, 6, 8];
        V::insert(&mut v, 7, 2);
        assert!(v == vector[5, 6, 7, 8], 0);
    }

    #[test]
    fun insert_at_end() {
        let mut v = vector[];
        V::insert(&mut v, 6, 0);
        assert!(v == vector[6], 0);

        V::insert(&mut v, 7, 1);
        assert!(v == vector[6, 7], 0);
    }

    #[test]
    #[expected_failure(abort_code = V::EINDEX_OUT_OF_BOUNDS)]
    fun insert_out_of_range() {
        let mut v = vector[7];
        V::insert(&mut v, 6, 2);
    }

    #[test]
    fun size_limit_ok() {
        let mut v = V::empty();
        let mut i = 0;
        // Limit is currently 1024 * 54
        let max_len = 1024 * 53;

        while (i < max_len) {
            V::push_back(&mut v, i);
            i = i + 1;
        };
    }

    #[test]
    #[expected_failure(out_of_gas, location = Self)]
    fun size_limit_fail() {
        let mut v = V::empty();
        let mut i = 0;
        // Choose value beyond limit
        let max_len = 1024 * 1024;

        while (i < max_len) {
            V::push_back(&mut v, i);
            i = i + 1;
        };
    }
}
