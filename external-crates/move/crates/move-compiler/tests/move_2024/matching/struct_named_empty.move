module 0x42::m {
    public struct A { }

    fun t00(s: A): u64 {
        match (s) {
            A { } => 0,
        }
    }

    fun t01(s: &A, default: &u64): &u64 {
        match (s) {
            A { } => default,
        }
    }

    fun t02(s: &mut A, default: &mut u64): &mut u64 {
        match (s) {
            A { } => default,
        }
    }
}
