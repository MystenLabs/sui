module 0x42::M {
    struct MyTable1<phantom K, phantom V> {}

    native fun new<K, V>(): MyTable1<K, V>;
    native fun destroy_empty<K, V>(t: MyTable1<K, V>);
    native fun borrow_mut<K, V>(t: &mut MyTable1<K, V>, k: K): &mut V;
    native fun length<K, V>(t: &MyTable1<K, V>): u64;

    spec native fun spec_len<K, V>(t: MyTable1<K, V>): num;
    spec native fun spec_set<K, V>(t: MyTable1<K, V>, k: K, v: V): MyTable1<K, V>;
    spec native fun spec_get<K, V>(t: MyTable1<K, V>, k: K): V;

    spec MyTable1 {
        pragma intrinsic = map,
            map_new = new,
            map_destroy_empty = destroy_empty,
            map_borrow_mut = borrow_mut,
            map_len = length,
            map_spec_len = spec_len,
            map_spec_get = spec_get,
            map_spec_set = spec_set;
    }

    struct MyTable2<phantom K, phantom V> {}

    native fun new2<K, V>(): MyTable2<K, V>;
    native fun contains<K, V>(t: &MyTable2<K, V>, k: K): bool;
    native fun borrow<K, V>(t: &MyTable2<K, V>, k: K): &V;
    native fun remove<K, V>(t: &mut MyTable2<K, V>, k: K): V;

    spec native fun spec_len2<K, V>(t: MyTable2<K, V>): num;
    spec native fun spec_del<K, V>(t: MyTable2<K, V>): num;
    spec native fun spec_has_key<K, V>(t: MyTable2<K, V>, k: K): bool;

    spec MyTable2 {
        pragma intrinsic = map,
            map_new = new2,
            map_has_key = contains,
            map_borrow = borrow,
            map_del_must_exist = remove,
            map_spec_len = spec_len2,
            map_spec_del = spec_del,
            map_spec_has_key = spec_has_key;
    }
}
