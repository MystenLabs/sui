module 0x12::tc2 {
    public fun foo(arg0: u64) : u64 {
        let v0 = arg0 + 1;
        let v1 = v0;
        if (v0 == 2) {
            v1 = v0 - 2;
        };
        v1 + 11
    }

    // decompiled from Move bytecode v6
}
