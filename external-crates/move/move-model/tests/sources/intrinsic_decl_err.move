module 0x42::M {
    struct MyTable<phantom K, phantom V> {}
    struct MyTable1<phantom K, phantom V> {}

    native fun new<K, V>(): MyTable1<K, V>;
    native fun destroy_empty<K, V>(t: MyTable1<K, V>);
    native fun borrow_mut<K, V>(t: &mut MyTable1<K, V>, k: K): &mut V;
    native fun length<K, V>(t: &MyTable1<K, V>): u64;

    spec native fun spec_len<K, V>(t: MyTable1<K, V>): num;
    spec native fun spec_set<K, V>(t: MyTable1<K, V>, k: K, v: V): MyTable1<K, V>;
    spec native fun spec_get<K, V>(t: MyTable1<K, V>, k: K): V;

    spec MyTable1 {
        // expect failure
        pragma intrinsic = 0x42::M::MyTable;

        // expect failure
        pragma intrinsic = no_such_map;

        // expect failure
        pragma intrinsic = map,
            map_no_such_fun = new;

        // expect failure
        pragma intrinsic = map,
            map_len = true;

        // expect failure
        pragma intrinsic = map,
            map_len = 0x1::signer::address_of;

        // expect failure
        pragma intrinsic = map,
            map_len = no_such_move_fun;

        // expect failure
        pragma intrinsic = map,
            map_spec_len = no_such_spec_fun;

        // expect failure
        pragma intrinsic = map,
            map_len = spec_len;

        // expect failure
        pragma intrinsic = map,
            map_len = length,
            map_borrow_mut = length;

        // expect failure
        pragma intrinsic = map,
            map_spec_len = spec_len,
            map_spec_set = spec_len;
    }
}
