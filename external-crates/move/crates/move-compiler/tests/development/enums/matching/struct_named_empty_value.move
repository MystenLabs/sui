module 0x42::m {
    public struct A { }

    fun t00(s: A): u64 {
        match (s) {
            A { } => 0,
        }
    }

}
