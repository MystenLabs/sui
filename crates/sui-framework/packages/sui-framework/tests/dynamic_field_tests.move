// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::dynamic_field_tests {
    use sui::dynamic_field::{add, exists_with_type, borrow, borrow_mut, remove};
    use sui::object;
    use sui::test_scenario as ts;

    #[test]
    fun simple_all_functions() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let id = ts::new_object(&mut scenario);
        // add fields
        add<u64, u64>(&mut id, 0, 0);
        add<vector<u8>, u64>(&mut id, b"", 1);
        add<bool, u64>(&mut id, false, 2);
        // check they exist
        assert!(exists_with_type<u64, u64>(&id, 0), 0);
        assert!(exists_with_type<vector<u8>, u64>(&id, b""), 0);
        assert!(exists_with_type<bool, u64>(&id, false), 0);
        // check the values
        assert!(*borrow(&id, 0) == 0, 0);
        assert!(*borrow(&id, b"") == 1, 0);
        assert!(*borrow(&id, false) == 2, 0);
        // mutate them
        *borrow_mut(&mut id, 0) = 3 + *borrow(&id, 0);
        *borrow_mut(&mut id, b"") = 4 + *borrow(&id, b"");
        *borrow_mut(&mut id, false) = 5 + *borrow(&id, false);
        // check the new value
        assert!(*borrow(&id, 0) == 3, 0);
        assert!(*borrow(&id, b"") == 5, 0);
        assert!(*borrow(&id, false) == 7, 0);
        // remove the value and check it
        assert!(remove(&mut id, 0) == 3, 0);
        assert!(remove(&mut id, b"") == 5, 0);
        assert!(remove(&mut id, false) == 7, 0);
        // verify that they are not there
        assert!(!exists_with_type<u64, u64>(&id, 0), 0);
        assert!(!exists_with_type<vector<u8>, u64>(&id, b""), 0);
        assert!(!exists_with_type<bool, u64>(&id, false), 0);
        ts::end(scenario);
        object::delete(id);
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun add_duplicate() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let id = ts::new_object(&mut scenario);
        add<u64, u64>(&mut id, 0, 0);
        add<u64, u64>(&mut id, 0, 1);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldAlreadyExists)]
    fun add_duplicate_mismatched_type() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let id = ts::new_object(&mut scenario);
        add<u64, u64>(&mut id, 0, 0u64);
        add<u64, u8>(&mut id, 0, 1u8);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let id = ts::new_object(&mut scenario);
        borrow<u64, u64>(&id, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
    fun borrow_wrong_type() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let id = ts::new_object(&mut scenario);
        add(&mut id, 0, 0);
        borrow<u64, u8>(&id, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun borrow_mut_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let id = ts::new_object(&mut scenario);
        borrow_mut<u64, u64>(&mut id, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
    fun borrow_mut_wrong_type() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let id = ts::new_object(&mut scenario);
        add(&mut id, 0, 0);
        borrow_mut<u64, u8>(&mut id, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldDoesNotExist)]
    fun remove_missing() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let id = ts::new_object(&mut scenario);
        remove<u64, u64>(&mut id, 0);
        abort 42
    }

    #[test]
    #[expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
    fun remove_wrong_type() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let id = ts::new_object(&mut scenario);
        add(&mut id, 0, 0);
        remove<u64, u8>(&mut id, 0);
        abort 42
    }

    #[test]
    fun sanity_check_exists() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let id = ts::new_object(&mut scenario);
        assert!(!exists_with_type<u64, u64>(&id, 0), 0);
        add(&mut id, 0, 0);
        assert!(exists_with_type<u64, u64>(&id, 0), 0);
        assert!(!exists_with_type<u64, u8>(&id, 0), 0);
        ts::end(scenario);
        object::delete(id);
    }

    // should be able to do delete a UID even though it has a dynamic field
    #[test]
    fun delete_uid_with_fields() {
        let sender = @0x0;
        let scenario = ts::begin(sender);
        let id = ts::new_object(&mut scenario);
        add(&mut id, 0, 0);
        assert!(exists_with_type<u64, u64>(&id, 0), 0);
        ts::end(scenario);
        object::delete(id);
    }
}
