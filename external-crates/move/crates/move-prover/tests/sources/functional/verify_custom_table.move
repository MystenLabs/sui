// Similar to `verify_table.move`, but against a custom implementation.
module 0x42::table {
    struct Table<phantom K: copy + drop, phantom V> has store {}
    spec Table {
        pragma intrinsic = map,
        map_new = new,
        map_destroy_empty = destroy_empty,
        map_len = length,
        map_is_empty = empty,
        map_has_key = contains,
        map_add_no_override = add,
        map_add_override_if_exists = upsert,
        map_del_must_exist = remove,
        map_del_return_key = remove_return_key,
        map_borrow = borrow,
        map_borrow_mut = borrow_mut,
        map_spec_new = spec_new,
        map_spec_get = spec_get,
        map_spec_set = spec_set,
        map_spec_del = spec_remove,
        map_spec_len = spec_len,
        map_spec_has_key = spec_contains;
    }

    public native fun new<K: copy + drop, V: store>(): Table<K, V>;
    public native fun destroy_empty<K: copy + drop, V>(table: Table<K, V>);
    public native fun add<K: copy + drop, V>(table: &mut Table<K, V>, key: K, val: V);
    public native fun upsert<K: copy + drop, V>(table: &mut Table<K, V>, key: K, val: V);
    public native fun borrow<K: copy + drop, V>(table: &Table<K, V>, key: K): &V;
    public native fun borrow_mut<K: copy + drop, V>(table: &mut Table<K, V>, key: K): &mut V;
    public native fun length<K: copy + drop, V>(table: &Table<K, V>): u64;
    public native fun empty<K: copy + drop, V>(table: &Table<K, V>): bool;
    public native fun remove<K: copy + drop, V>(table: &mut Table<K, V>, key: K): V;
    public native fun remove_return_key<K: copy + drop, V>(table: &mut Table<K, V>, key: K): (K, V);
    public native fun contains<K: copy + drop, V>(table: &Table<K, V>, key: K): bool;

    spec native fun spec_new<K, V>(): Table<K, V>;
    spec native fun spec_len<K, V>(t: Table<K, V>): num;
    spec native fun spec_contains<K, V>(t: Table<K, V>, k: K): bool;
    spec native fun spec_set<K, V>(t: Table<K, V>, k: K, v: V): Table<K, V>;
    spec native fun spec_remove<K, V>(t: Table<K, V>, k: K): Table<K, V>;
    spec native fun spec_get<K, V>(t: Table<K, V>, k: K): V;
}

module 0x42::VerifyTable {
    use 0x42::table::{Self, Table};
    use 0x42::table::{spec_new, spec_get, spec_set, spec_len, spec_contains};

    // TODO: test precise aborts behavior of all table functions

    fun add(): Table<u8, u64> {
        let t = table::new<u8, u64>();
        table::add(&mut t, 1, 2);
        table::add(&mut t, 2, 3);
        table::add(&mut t, 3, 4);
        t
    }
    spec add {
        ensures spec_contains(result, 1) && spec_contains(result, 2) && spec_contains(result, 3);
        ensures spec_len(result) == 3;
        ensures spec_get(result, 1) == 2;
        ensures spec_get(result, 2) == 3;
        ensures spec_get(result, 3) == 4;
    }

    fun add_fail(): Table<u8, u64> {
        let t = table::new<u8, u64>();
        table::add(&mut t, 1, 2);
        table::add(&mut t, 2, 3);
        table::add(&mut t, 3, 4);
        t
    }
    spec add_fail {
        ensures spec_get(result, 1) == 1;
    }

    fun add_fail_exists(k1: u8, k2: u8): Table<u8, u64> {
        let t = table::new<u8, u64>();
        table::add(&mut t, k1, 2);
        table::add(&mut t, k2, 3);
        t
    }
    spec add_fail_exists {
        aborts_if k1 == k2;
    }

    fun add_override_if_exists(k1: u8, k2: u8): Table<u8, u64> {
        let t = table::new<u8, u64>();
        table::add(&mut t, k1, 2);
        table::upsert(&mut t, k2, 3);
        t
    }
    spec add_override_if_exists {
        aborts_if false;
        ensures spec_get(result, k2) == 3;
        ensures (k1 != k2) ==> spec_get(result, k1) == 2;
    }

    fun remove(): Table<u8, u64> {
        let t = add();
        table::remove(&mut t, 2);
        t
    }
    spec remove {
        ensures spec_contains(result, 1) && spec_contains(result, 3);
        ensures spec_len(result) == 2;
        ensures spec_get(result, 1) == 2;
        ensures spec_get(result, 3) == 4;
    }

    fun remove_return_key(): Table<u8, u64> {
        let t = add();
        let (k, v) = table::remove_return_key(&mut t, 2);
        spec {
            assert (k == 2) && (v == 3);
        };
        t
    }
    spec remove_return_key {
        ensures spec_contains(result, 1) && spec_contains(result, 3);
        ensures spec_len(result) == 2;
        ensures spec_get(result, 1) == 2;
        ensures spec_get(result, 3) == 4;
    }

    fun contains_and_length(): (bool, bool, u64, Table<u8, u64>) {
        let t = table::new<u8, u64>();
        table::add(&mut t, 1, 2);
        table::add(&mut t, 2, 3);
        (table::contains(&t, 1), table::contains(&t, 3), table::length(&t), t)
    }
    spec contains_and_length {
        ensures result_1 == true;
        ensures result_2 == false;
        ensures result_3 == 2;
    }

    fun borrow(): (u64, Table<u8, u64>) {
        let t = table::new<u8, u64>();
        table::add(&mut t, 1, 2);
        let r = table::borrow(&t, 1);
        (*r, t)
    }
    spec borrow {
        ensures result_1 == 2;
        ensures spec_len(result_2) == 1;
        ensures spec_get(result_2, 1) == 2;
    }

    fun borrow_mut(): Table<u8, u64> {
        let t = table::new<u8, u64>();
        table::add(&mut t, 1, 2);
        table::add(&mut t, 2, 3);
        let r = table::borrow_mut(&mut t, 1);
        *r = 4;
        t
    }
    spec borrow_mut {
        ensures spec_contains(result, 1) && spec_contains(result, 2);
        ensures spec_len(result) == 2;
        ensures spec_get(result, 1) == 4;
        ensures spec_get(result, 2) == 3;
    }

    fun create_empty(): Table<u8, u64> {
        table::new()
    }
    spec create_empty {
        ensures result == spec_new();
    }

    fun create_and_insert(): Table<u8, u64> {
        let t = table::new<u8, u64>();
        table::add(&mut t, 1, 2);
        t
    }
    spec create_and_insert {
        ensures result == spec_set<u8, u64>(spec_new(), 1, 2);
    }

    fun create_and_insert_fail_due_to_typed_key_encoding(): Table<u8, u64> {
        let t = table::new<u8, u64>();
        table::add(&mut t, 1, 2);
        t
    }
    spec create_and_insert_fail_due_to_typed_key_encoding {
        // TODO: this will fail potentially due to an error in type inference.
        // `spec_set` should receive a type parameter of `<u8, u64>`, (see the
        // example above) but the derived type is `<u256, u256>`.
        ensures result == spec_set(spec_new(), 1, 2);
    }

    fun create_and_insert_fail1(): Table<u8, u64> {
        let t = table::new<u8, u64>();
        table::add(&mut t, 1, 1);
        t
    }
    spec create_and_insert_fail1 {
        ensures result == spec_new();
    }

    fun create_and_insert_fail2(): Table<u8, u64> {
        let t = table::new<u8, u64>();
        table::add(&mut t, 1, 1);
        t
    }
    spec create_and_insert_fail2 {
        ensures result == spec_set(spec_new(), 1, 2);
    }

    // ====================================================================================================
    // Tables with structured keys

    struct Key has copy, drop {
        v: vector<u8>       // Use a vector so we do not have extensional equality
    }

    struct R {
        t: Table<Key, u64>
    }

    fun make_R(): R {
        let t = table::new<Key, u64>();
        table::add(&mut t, Key{v: vector[1, 2]}, 22);
        table::add(&mut t, Key{v: vector[2, 3]}, 23);
        R{t}
    }

    fun add_R(): R {
        make_R()
    }
    spec add_R {
        let k1 = Key{v: concat(vec(1u8), vec(2u8))};
        let k2 = Key{v: concat(vec(2u8), vec(3u8))};
        ensures spec_len(result.t) == 2;
        ensures spec_contains(result.t, k1) && spec_contains(result.t, k2);
        ensures spec_get(result.t, k1) == 22;
        ensures spec_get(result.t, k2) == 23;
    }

    fun add_R_fail(): R {
        make_R()
    }
    spec add_R_fail {
        let k1 = Key{v: concat(vec(1u8), vec(2u8))};
        let k2 = Key{v: concat(vec(2u8), vec(3u8))};
        ensures spec_len(result.t) == 2;
        ensures spec_contains(result.t, k1) && spec_contains(result.t, k2);
        ensures spec_get(result.t, k1) == 23;
        ensures spec_get(result.t, k2) == 22;
    }

    fun borrow_mut_R(): R {
        let r = make_R();
        let x = table::borrow_mut(&mut r.t, Key{v: vector[1, 2]});
        *x = *x * 2;
        r
    }
    spec borrow_mut_R {
        let k1 = Key{v: concat(vec(1u8), vec(2u8))};
        let k2 = Key{v: concat(vec(2u8), vec(3u8))};
        ensures spec_len(result.t) == 2;
        ensures spec_get(result.t, k1) == 44;
        ensures spec_get(result.t, k2) == 23;
    }
}
