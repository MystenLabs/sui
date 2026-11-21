module 0x42::m {

    // Both bool values are covered, so the chain needs no default arm.
    fun saturated(b: bool): u64 {
        match (b) {
            true => 10,
            false => 20,
        }
    }

}
