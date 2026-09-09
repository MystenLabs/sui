module 0x42::m {

    // The if/else chain lowered from these arms has no syntax of its own.
    fun literal(x: u64): u64 {
        match (x) {
            1 => 10,
            2 => 20,
            _ => 30,
        }
    }

}
