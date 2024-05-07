// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::option_tests {
    use std::option;

    #[test]
    fun option_none_is_none() {
        let none = option::none<u64>();
        assert!(none.is_none(), 0);
        assert!(!none.is_some(), 1);
    }

    #[test]
    fun option_some_is_some() {
        let some = option::some(5);
        assert!(!some.is_none(), 0);
        assert!(some.is_some(), 1);
    }

    #[test]
    fun option_contains() {
        let none = option::none<u64>();
        let some = option::some(5);
        let some_other = option::some(6);
        assert!(some.contains(&5), 0);
        assert!(some_other.contains(&6), 1);
        assert!(!none.contains(&5), 2);
        assert!(!some_other.contains(&5), 3);
    }

    #[test]
    fun option_borrow_some() {
        let some = option::some(5);
        let some_other = option::some(6);
        assert!(*some.borrow() == 5, 3);
        assert!(*some_other.borrow() == 6, 4);
    }

    #[test]
    #[expected_failure(abort_code = option::EOPTION_NOT_SET)]
    fun option_borrow_none() {
        option::none<u64>().borrow();
    }

    #[test]
    fun borrow_mut_some() {
        let mut some = option::some(1);
        let ref = some.borrow_mut();
        *ref = 10;
        assert!(*some.borrow() == 10, 0);
    }

    #[test]
    #[expected_failure(abort_code = option::EOPTION_NOT_SET)]
    fun borrow_mut_none() {
        option::none<u64>().borrow_mut();
    }

    #[test]
    fun borrow_with_default() {
        let none = option::none<u64>();
        let some = option::some(5);
        assert!(*some.borrow_with_default(&7) == 5, 0);
        assert!(*none.borrow_with_default(&7) == 7, 1);
    }

    #[test]
    fun get_with_default() {
        let none = option::none<u64>();
        let some = option::some(5);
        assert!(option::get_with_default(&some, 7) == 5, 0);
        assert!(option::get_with_default(&none, 7) == 7, 1);
    }

    #[test]
    fun extract_some() {
        let mut opt = option::some(1);
        assert!(opt.extract() == 1, 0);
        assert!(opt.is_none(), 1);
    }

    #[test]
    #[expected_failure(abort_code = option::EOPTION_NOT_SET)]
    fun extract_none() {
        option::none<u64>().extract();
    }

    #[test]
    fun swap_some() {
        let mut some = option::some(5);
        assert!(some.swap(1) == 5, 0);
        assert!(*some.borrow() == 1, 1);
    }

    #[test]
    fun swap_or_fill_some() {
        let mut some = option::some(5);
        assert!(some.swap_or_fill(1) == option::some(5), 0);
        assert!(*some.borrow() == 1, 1);
    }

    #[test]
    fun swap_or_fill_none() {
        let mut none = option::none();
        assert!(none.swap_or_fill(1) == option::none(), 0);
        assert!(*none.borrow() == 1, 1);
    }

    #[test]
    #[expected_failure(abort_code = option::EOPTION_NOT_SET)]
    fun swap_none() {
        option::none<u64>().swap(1);
    }

    #[test]
    fun fill_none() {
        let mut none = option::none<u64>();
        none.fill(3);
        assert!(none.is_some(), 0);
        assert!(*none.borrow() == 3, 1);
    }

    #[test]
    #[expected_failure(abort_code = option::EOPTION_IS_SET)]
    fun fill_some() {
        option::some(3).fill(0);
    }

    #[test]
    fun destroy_with_default() {
        assert!(option::none<u64>().destroy_with_default(4) == 4, 0);
        assert!(option::some(4).destroy_with_default(5) == 4, 1);
    }

    #[test]
    fun destroy_some() {
        assert!(option::some(4).destroy_some() == 4, 0);
    }

    #[test]
    #[expected_failure(abort_code = option::EOPTION_NOT_SET)]
    fun destroy_some_none() {
        option::none<u64>().destroy_some();
    }

    #[test]
    fun destroy_none() {
        option::none<u64>().destroy_none();
    }

    #[test]
    #[expected_failure(abort_code = option::EOPTION_IS_SET)]
    fun destroy_none_some() {
        option::some<u64>(0).destroy_none();
    }

    #[test]
    fun into_vec_some() {
        let mut v = option::some<u64>(0).to_vec();
        assert!(v.length() == 1, 0);
        let x = v.pop_back();
        assert!(x == 0, 1);
    }

    #[test]
    fun into_vec_none() {
        let v: vector<u64> = option::none().to_vec();
        assert!(v.is_empty(), 0);
    }
}
