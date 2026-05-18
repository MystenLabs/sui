module 0x42::m {
    public struct S { x: u64 }

    fun t(s: S): u64 {
        match (s) {
            S { x } | S { bogus: x } => x,
        }
    }
}
