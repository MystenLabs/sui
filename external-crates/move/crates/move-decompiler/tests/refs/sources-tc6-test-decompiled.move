module 0x12::tc6 {
    public fun foo(arg0: u64) : u64 {
        let v0 = arg0 + 1;
        if (v0 == 2) {
            return 2 - v0
        };
        v0 - 5
    }

    // decompiled from Move bytecode v6
}
