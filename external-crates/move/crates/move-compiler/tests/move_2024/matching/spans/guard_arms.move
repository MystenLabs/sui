module 0x42::m {

    // The generated branch for a guard tests user-written syntax.
    fun guarded(x: u64, y: u64): u64 {
        match (x) {
            1 if (y > 0) => 10,
            _ => 20,
        }
    }

}
