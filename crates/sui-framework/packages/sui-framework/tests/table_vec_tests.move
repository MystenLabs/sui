// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::table_vec_tests {
    use sui::table_vec;
    use sui::test_scenario;

    const TEST_SENDER_ADDR: address = @0x1;

    #[test]
    fun simple_all_functions() {
        let mut scenario = test_scenario::begin(TEST_SENDER_ADDR);

        let mut table_vec = table_vec::empty<u64>(scenario.ctx());
        assert!(table_vec.length() == 0);

        table_vec.push_back(7);
        let mut_value = &mut table_vec[0];
        *mut_value = *mut_value + 1;
        let value = &table_vec[0];
        assert!(*value == 8);

        table_vec.push_back(5);
        table_vec.swap(0, 1);

        let value = table_vec.swap_remove(0);
        assert!(value == 5);

        let value = table_vec.pop_back();
        assert!(value == 8);
        table_vec.destroy_empty();
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = sui::table_vec::ETableNonEmpty)]
    fun destroy_non_empty_aborts() {
        let mut scenario = test_scenario::begin(TEST_SENDER_ADDR);
        let table_vec = table_vec::singleton(1, scenario.ctx());
        table_vec.destroy_empty();
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = sui::table_vec::EIndexOutOfBound)]
    fun pop_back_empty_aborts() {
        let mut scenario = test_scenario::begin(TEST_SENDER_ADDR);
        let mut table_vec = table_vec::empty<u64>(scenario.ctx());
        table_vec.pop_back();
        table_vec.destroy_empty();
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = sui::table_vec::EIndexOutOfBound)]
    fun borrow_out_of_bounds_aborts() {
        let mut scenario = test_scenario::begin(TEST_SENDER_ADDR);
        let table_vec = table_vec::singleton(1, scenario.ctx());
        let _ = &table_vec[77];
        table_vec.destroy_empty();
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = sui::table_vec::EIndexOutOfBound)]
    fun borrow_mut_out_of_bounds_aborts() {
        let mut scenario = test_scenario::begin(TEST_SENDER_ADDR);
        let mut table_vec = table_vec::singleton(1, scenario.ctx());
        let _ = &mut table_vec[77];
        table_vec.destroy_empty();
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = sui::table_vec::EIndexOutOfBound)]
    fun swap_out_of_bounds_aborts() {
        let mut scenario = test_scenario::begin(TEST_SENDER_ADDR);
        let mut table_vec = table_vec::singleton(1, scenario.ctx());
        table_vec.swap(0, 77);
        table_vec.destroy_empty();
        scenario.end();
    }

    #[test]
    fun swap_same_index_succeeds() {
        let mut scenario = test_scenario::begin(TEST_SENDER_ADDR);
        let mut table_vec = table_vec::singleton(1, scenario.ctx());
        table_vec.swap(0, 0);
        table_vec.pop_back();
        table_vec.destroy_empty();
        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = sui::table_vec::EIndexOutOfBound)]
    fun swap_same_index_out_of_bounds_aborts() {
        let mut scenario = test_scenario::begin(TEST_SENDER_ADDR);
        let mut table_vec = table_vec::singleton(1, scenario.ctx());
        table_vec.swap(77, 77);
        table_vec.destroy_empty();
        scenario.end();
    }
}
