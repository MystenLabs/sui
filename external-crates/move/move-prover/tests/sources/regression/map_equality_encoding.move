module 0x42::map_equality {
    use extensions::table;

    fun test(_t: &mut table::Table<address, u64>, _k: address) {}
    spec test {
        let old_v = table::spec_get(_t, _k);
        ensures table::spec_contains(old(_t), _k) ==> _t == table::spec_set(old(_t), _k, old_v + 0);
    }
}
