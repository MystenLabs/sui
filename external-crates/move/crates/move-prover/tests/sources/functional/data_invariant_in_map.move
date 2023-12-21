module 0x42::data_inv_in_map {
    use extensions::table;

    struct S has store {
        value: u64
    }
    spec S {
        invariant value != 0;
    }

    struct R has key {
        map: table::Table<address, S>
    }

    fun no_violation() acquires R {
        let t = &mut borrow_global_mut<R>(@0x42).map;
        table::add(t, @0x43, S { value: 1 });
    }

    fun violation_1() acquires R {
        let t = &mut borrow_global_mut<R>(@0x42).map;
        let s = table::borrow_mut(t, @0x43);
        *&mut s.value = 0;
    }
}
