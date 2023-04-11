/// Specifications of the `Table` module.
spec extensions::table {

    // Make most of the public API intrinsic. Those functions have custom specifications in the prover.

    spec Table {
        pragma intrinsic = map,
            map_new = new,
            map_destroy_empty = destroy_empty,
            map_len = length,
            map_is_empty = empty,
            map_has_key = contains,
            map_add_no_override = add,
            map_del_must_exist = remove,
            map_borrow = borrow,
            map_borrow_mut = borrow_mut,
            map_spec_get = spec_get,
            map_spec_set = spec_set,
            map_spec_del = spec_remove,
            map_spec_len = spec_len,
            map_spec_has_key = spec_contains;
    }

    // Specification functions for tables

    spec native fun spec_len<K, V>(t: Table<K, V>): num;
    spec native fun spec_contains<K, V>(t: Table<K, V>, k: K): bool;
    spec native fun spec_set<K, V>(t: Table<K, V>, k: K, v: V): Table<K, V>;
    spec native fun spec_remove<K, V>(t: Table<K, V>, k: K): Table<K, V>;
    spec native fun spec_get<K, V>(t: Table<K, V>, k: K): V;
}
