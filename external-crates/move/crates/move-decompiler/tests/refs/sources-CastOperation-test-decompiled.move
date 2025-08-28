module 0xbadbadbad::CastOperation {
    fun test_cast1(arg0: u64) : u256 {
        (arg0 as u256)
    }

    fun test_cast2(arg0: u64) : u256 {
        ((arg0 as u128) as u256)
    }

    // decompiled from Move bytecode v6
}
