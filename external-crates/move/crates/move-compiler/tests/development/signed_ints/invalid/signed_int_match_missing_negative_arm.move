// Non-exhaustive matches over signed subjects: the counterexample reports must render the
// negative literals from the existing arms correctly.
module 0x42::m {
    fun missing_arms_i8(x: i8): u64 {
        match (x) {
            -128i8 => 0,
            -1i8 => 1,
            0i8 => 2,
        }
    }

    fun missing_arms_i256(x: i256): u64 {
        match (x) {
            -1i256 => 0,
        }
    }
}
