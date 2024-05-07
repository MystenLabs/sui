module 0x42::M {

    fun foo(_: &u64) {}

    #[allow(dead_code)]
    fun t(cond: bool) { 'a: {
        1 + if (cond) 0 else 'a: { 1 } + 2;
        1 + 'a: loop {} + 2;
        1 + return 'a + 0;

        foo(&if (cond) 0 else 1);
        foo(&'a: loop {});
        foo(&return 'a);
        foo(&abort 0);
    } }
}
