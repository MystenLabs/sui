/// This module defines Table for EVM.
module Evm::Table {
    native struct Table<phantom K, phantom V> has store;

    /// Create an empty Table.
    native public fun empty<K, V>(): Table<K, V>;

    /// Acquire an immutable reference to the value which `key` maps to.
    /// Aborts if there is no entry for `key`.
    native public fun borrow<K, V>(table: &Table<K, V>, key: &K): &V;

    /// Acquire a mutable reference to the value which `key` maps to.
    /// Aborts if there is no entry for `key`.
    native public fun borrow_mut<K, V>(table: &mut Table<K, V>, key: &K): &mut V;

    /// Acquire a mutable reference to the value which `key` maps to.
    /// Insert the pair (`key`, `default`) first if there is no entry for `key`.
    public fun borrow_mut_with_default<K, V: drop>(table: &mut Table<K, V>, key: &K, default_value: V): &mut V {
        if (!contains(table, key)) {
            insert(table, key, default_value)
        };
        borrow_mut(table, key)
    }

    /// Remove from `table` and return the value which `key` maps to.
    /// Aborts if there is no entry for `key`.
    native public fun remove<K, V>(table: &mut Table<K, V>, key: &K): V;

    /// Returns true iff `table` contains an entry for `key`.
    native public fun contains<K, V>(table: &Table<K, V>, key: &K): bool;

    /// Insert the pair (`key`, `val`) to `table`.
    /// Aborts if there is already an entry for `key`.
    native public fun insert<K, V>(table: &mut Table<K, V>, key: &K, val: V);
}
