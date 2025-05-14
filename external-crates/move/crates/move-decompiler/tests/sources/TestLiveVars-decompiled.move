module 0x12::TestLiveVars {
    struct R has copy, drop {
        x: u64,
    }

    fun test1(arg0: &R) : u64 {
        arg0.x
    }

    fun test2(arg0: bool) : u64 {
        let v0 = R{x: 3};
        let v1 = R{x: 4};
        let v2 = &v0;
        if (arg0) {
            v2 = &v1;
        };
        test1(v2)
    }

    fun test3(arg0: u64, arg1: &R) : u64 {
        let v0 = R{x: 3};
        let v1 = R{x: 4};
        while (0 < arg0) {
            if (arg0 / 2 == 0) {
                arg1 = &v0;
            } else {
                arg1 = &v1;
            };
            arg0 = arg0 - 1;
        };
        test1(arg1)
    }

    // decompiled from Move bytecode v6
}
