module 0x42::M {

    fun foo(_: &u64) {}

    fun t(cond: bool) {
        1 + if (cond) 0 else { 1 } + 2;
        1 + loop {} + 2;
        1 + return + 0;

        foo(&if (cond) 0 else 1);
        foo(&loop {});
        foo(&return);
        foo(&abort 0);
    }
}
