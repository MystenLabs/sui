module 0x12::tc5 {
    public fun foo(arg0: u64) : u64 {
        let v0 = arg0 + 1;
        if (v0 == 2) {
            return 11 - v0 * 2 + arg0 + 2 - 1
        };
        5 - v0
    }

    // decompiled from Move bytecode v6
}
