module 0x12::tc9 {
    public fun foo() : u64 {
        let v0 = 0;
        loop {
            v0 = v0 + 1;
            if (v0 / 2 == 0) {
                continue
            };
            if (v0 == 5) {
                break
            };
            v0 = v0 + 69 + v0;
        };
        v0 + 99
    }

    // decompiled from Move bytecode v6
}
