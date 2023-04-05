module 0x42::table_contains_to_length {
    use extensions::table;

    fun test(_t: &mut table::Table<address, u64>, _k: address) {}
    spec test {
        ensures table::spec_contains(old(_t), _k) ==> table::spec_len(old(_t)) > 0;
        ensures table::spec_contains(old(_t), _k) ==> table::spec_len(_t) > 0;
    }
}
