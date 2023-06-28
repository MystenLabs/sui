// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::vec_set_tests {
    use std::vector;
    use sui::vec_set;

    #[test]
    #[expected_failure(abort_code = vec_set::EKeyAlreadyExists)]
    fun duplicate_key_abort() {
        let m = vec_set::empty();
        vec_set::insert(&mut m, 1);
        vec_set::insert(&mut m, 1);
    }

    #[test]
    #[expected_failure(abort_code = vec_set::EKeyDoesNotExist)]
    fun nonexistent_key_remove() {
        let m = vec_set::empty();
        vec_set::insert(&mut m, 1);
        let k = 2;
        vec_set::remove(&mut m, &k);
    }

    #[test]
    fun smoke() {
        let m = vec_set::empty();
        let i = 0;
        while (i < 10) {
            let k = i + 2;
            vec_set::insert(&mut m, k);
            i = i + 1;
        };
        assert!(!vec_set::is_empty(&m), 0);
        assert!(vec_set::size(&m) == 10, 1);
        let i = 0;
        // make sure the elements are as expected in all of the getter APIs we expose
        while (i < 10) {
            let k = i + 2;
            assert!(vec_set::contains(&m, &k), 2);
            i = i + 1;
        };
        // remove all the elements
        let keys = vec_set::into_keys(copy m);
        let i = 0;
        while (i < 10) {
            let k = i + 2;
            vec_set::remove(&mut m, &k);
            assert!(*vector::borrow(&keys, i) == k, 9);
            i = i + 1;
        }
    }

    #[test]
    fun test_keys() {
        let m = vec_set::empty();
        vec_set::insert(&mut m, 1);
        vec_set::insert(&mut m, 2);
        vec_set::insert(&mut m, 3);

        assert!(vec_set::size(&m) == 3, 0);
        assert!(vec_set::keys(&m) == &vector[1, 2, 3], 1);
    }
}
