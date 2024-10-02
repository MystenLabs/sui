// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::vec_set_tests {
    use sui::vec_set;

    #[test]
    #[expected_failure(abort_code = vec_set::EKeyAlreadyExists)]
    fun duplicate_key_abort() {
        let mut m = vec_set::empty();
        m.insert(1);
        m.insert(1);
    }

    #[test]
    #[expected_failure(abort_code = vec_set::EKeyDoesNotExist)]
    fun nonexistent_key_remove() {
        let mut m = vec_set::empty();
        m.insert(1);
        let k = 2;
        m.remove(&k);
    }

    #[test]
    fun smoke() {
        let mut m = vec_set::empty();
        let mut i = 0;
        while (i < 10) {
            let k = i + 2;
            m.insert(k);
            i = i + 1;
        };
        assert!(!m.is_empty());
        assert!(m.size() == 10);
        let mut i = 0;
        // make sure the elements are as expected in all of the getter APIs we expose
        while (i < 10) {
            let k = i + 2;
            assert!(m.contains(&k));
            i = i + 1;
        };
        // remove all the elements
        let keys = (copy m).into_keys();
        let mut i = 0;
        while (i < 10) {
            let k = i + 2;
            m.remove(&k);
            assert!(keys[i] == k);
            i = i + 1;
        }
    }

    #[test]
    fun test_keys() {
        let mut m = vec_set::empty();
        m.insert(1);
        m.insert(2);
        m.insert(3);

        assert!(m.size() == 3);
        assert!(m.keys() == &vector[1, 2, 3]);
    }

    #[test, allow(lint(collection_equality))]
    fun round_trip() {
        let mut s = vec_set::empty();
        assert!(s == vec_set::from_keys(vector[]));
        let mut i = 0;
        while (i < 50) {
            let k = i + 2;
            s.insert(k);
            let s2 = vec_set::from_keys(s.into_keys());
            assert!(s == s2);
            i = i + 1;
        };
    }

    #[test, expected_failure(abort_code = vec_set::EKeyAlreadyExists)]
    fun from_keys_values_duplicate_key_abort() {
        vec_set::from_keys<u64>(vector[1, 0, 1]);
    }
}
