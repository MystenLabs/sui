module 0x12::test {
    struct Example has copy, drop {
        i: u64,
    }

    public fun print(arg0: u64) : u64 {
        let v0 = arg0 + 1;
        Example{i: arg0 + 2};
        let v1 = if (v0 < 10) {
            let v2 = v0 + 1;
            let v3 = if (v2 > 11) {
                1
            } else {
                2
            };
            v2 + v3
        } else {
            if (v0 == 11) {
                return 1
            };
            v0 + 2
        };
        if (v1 > 11) {
            abort 23
        };
        let v4 = v1 + 2;
        assert!(v4 < 13, v4 - 10);
        v4 + 1
    }

    // decompiled from Move bytecode v6
}
