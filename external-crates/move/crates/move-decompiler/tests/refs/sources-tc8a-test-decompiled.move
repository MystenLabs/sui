module 0x12::tc8a {
    public fun foo() : u64 {
        let v0 = 0;
        while (v0 < 5) {
            v0 = v0 + 1;
        };
        v0 + 2
    }

    // decompiled from Move bytecode v6
}
