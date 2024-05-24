module 0x42::m {
    public struct A { x: u64 }

    fun t00(s: A): u64 {
        match (s) {
            A { x: 0 } => 0,
            A { x } => x,
        }
    }

}
