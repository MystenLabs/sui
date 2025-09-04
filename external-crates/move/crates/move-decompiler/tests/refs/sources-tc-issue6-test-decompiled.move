module 0x12::tc_i6 {
    fun calculate(arg0: u256, arg1: u256, arg2: u256) : u256 {
        let v0 = arg1;
        if (arg2 > 1000) {
            arg1 = arg1 + 1;
        } else {
            arg1 = arg1 - 1;
        };
        if (arg1 > v0) {
            return arg0
        };
        arg1
    }

    // decompiled from Move bytecode v6
}
