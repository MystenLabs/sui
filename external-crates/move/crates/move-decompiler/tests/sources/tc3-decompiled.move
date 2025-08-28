module 0x12::tc3 {
    public fun foo(arg0: u64) : u64 {
        let v0 = arg0 + 1;
        let v1 = if (v0 == 2) {
            2 - v0
        } else {
            v0 - 5
        };
        v0 + v1 + 11
    }

    // decompiled from Move bytecode v6
}
