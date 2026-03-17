// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module a::m {
    public struct Map<K: copy + drop, V> has drop {
        keys: vector<K>,
        values: vector<V>,
    }

    public fun new<K: copy + drop, V>(keys: vector<K>, values: vector<V>): Map<K, V> {
        Map { keys, values }
    }
}

#[test_only]
extend module a::m {
    fun find_index<K: copy + drop>(keys: &vector<K>, key: &K): u64 {
        let mut i = 0;
        while (i < keys.length()) {
            if (&keys[i] == key) return i;
            i = i + 1;
        };
        abort 0
    }

    #[syntax(index)]
    fun borrow<K: copy + drop, V>(self: &Map<K, V>, key: &K): &V {
        let i = find_index(&self.keys, key);
        &self.values[i]
    }

    #[syntax(index)]
    fun borrow_mut<K: copy + drop, V>(self: &mut Map<K, V>, key: &K): &mut V {
        let i = find_index(&self.keys, key);
        &mut self.values[i]
    }

    #[test]
    fun test_generic_index() {
        let m = new(vector[1u64, 2, 3], vector[10u64, 20, 30]);
        assert!(&m[&1] == 10, 0);
        assert!(&m[&3] == 30, 1);
    }

    #[test]
    fun test_generic_index_mut() {
        let mut m = new(vector[1u64, 2, 3], vector[10u64, 20, 30]);
        assert!(&mut m[&2] == 20, 0);
    }
}
