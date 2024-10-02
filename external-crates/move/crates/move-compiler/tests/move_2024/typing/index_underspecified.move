module a::m;

public struct Table<phantom K: copy + drop + store, phantom V: store> has drop {
    size: u64,
}

#[syntax(index)]
public fun borrow<K: copy + drop + store, V: store>(_table: &mut Table<K, V>, _k: K): &mut V {
    abort 0
}

public fun index_by_reference<T: store>(table: &mut Table<u64, T>) {
    table[&1].push_back(3);
}
