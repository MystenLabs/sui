module 0x42::m {

    public struct S { a: u64 }

    // The invalid return must point at the match, not at a literal inside a pattern.
    fun literal_return(x: u64): u64 {
        let s = S { a: 1 };
        match (x) {
            1 => 10,
            _ => 20,
        }
    }

}
