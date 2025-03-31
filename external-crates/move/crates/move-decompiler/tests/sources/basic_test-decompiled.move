module 0x12::basic_test {
    struct R has key {
        x: u64,
        y: bool,
    }

    fun basic(arg0: u64, arg1: u64) : u64 {
        (arg0 + arg1) / arg0 + 1
    }

    // decompiled from Move bytecode v6
}
