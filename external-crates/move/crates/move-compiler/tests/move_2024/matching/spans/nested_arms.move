module 0x42::m {

    // An inner match must not be attributed to the arms of the outer one.
    fun nested(x: u64, y: u64): u64 {
        match (x) {
            1 => match (y) {
                10 => 100,
                _ => 200,
            },
            _ => 300,
        }
    }

}
