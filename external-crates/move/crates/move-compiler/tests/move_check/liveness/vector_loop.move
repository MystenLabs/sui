module a::m {

    use std::vector;
    use sui::table;

    struct ID has store, copy, drop {
        bytes: address,
    }

    struct UID has store {
        id: ID,
    }

    struct User has key, store { id: ID }

    struct Org {
        table: table::Table<ID, User>,
    }

    public fun delete(self: Org, users: &mut vector<User>) {
        let Org { table } = self;
        while (!table::is_empty(&table)) {
            user_delete(table::remove(&mut table, user_id(&vector::pop_back(users))));
        };
        table::destroy<ID, User>(table)
    }

    public fun user_delete(_user: User) {

    }

    public fun user_id(user: &User): ID {
        let User { id } = user;
        *id
    }

    public fun id_delete(id: UID) {
        let UID { id: ID { bytes } } = id;
        delete_impl(bytes)
    }

    /// Get the underlying `ID` of `obj`
    public fun id<T: key>(obj: &T): ID {
        borrow_uid(obj).id
    }

    /// Borrow the underlying `ID` of `obj`
    public fun borrow_id<T: key>(obj: &T): &ID {
        &borrow_uid(obj).id
    }

    native fun borrow_uid<T: key>(obj: &T): &UID;

    native fun delete_impl(id: address);

}

#[defines_primitive(vector)]
module std::vector {
    #[bytecode_instruction]
    native public fun pop_back<Element>(v: &mut vector<Element>): Element;
}

module sui::table {
    struct Table<phantom K: copy + drop + store, phantom V: store> {
        size: u64,
    }

    public fun new<K: copy + drop + store, V: store>(): Table<K, V> {
        abort 0
    }

    public fun destroy<K: copy + drop + store, V: store>(table: Table<K, V>) {
        let Table { size: _size } = table;
    }

    public fun remove<K: copy + drop + store, V: store>(_table: &mut Table<K, V>, _k: K): V {
        abort 0
    }

    public fun is_empty<K: copy + drop + store, V: store>(_table: &Table<K, V>): bool {
        false
    }

}
