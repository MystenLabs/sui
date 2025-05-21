module NamedAddr::CastOperation {
    fun test_cast1(a: u64): u256 {
        (a as u256)
    }

    fun test_cast2(a: u64): u256 {
        ((a as u128) as u256)
    }
}
