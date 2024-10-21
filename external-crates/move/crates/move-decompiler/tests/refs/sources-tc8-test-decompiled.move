module 0x23::tc8 {
    public fun foo() : u64 {
        let v0 = 0;
        let v1 = v0;
        let v2 = v0;
        while (v2 < 5) {
            let v3 = v2 + 1;
            v2 = v3;
            v1 = v1 - v3;
        };
        v2 + 2 + v1
    }

    // decompiled from Move bytecode v6
}
