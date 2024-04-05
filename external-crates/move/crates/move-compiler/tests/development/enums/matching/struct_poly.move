module 0x42::m {
    public struct A<T> { x: T }
    public struct B<T>(T)

    fun t00(s: A<u64>): u64 {
        match (s) {
            A { x: 0 } => 0,
            A { x } => x,
        }
    }

    fun t01(s: &A<u64>, default: &u64): &u64 {
        match (s) {
            A { x: 0 } => default,
            A { x } => x,
        }
    }

    fun t02(s: &mut A<u64>, default: &mut u64): &mut u64 {
        match (s) {
            A { x: 0 } => default,
            A { x } => x,
        }
    }

    fun t03(s: B<u64>): u64 {
        match (s) {
            B(0) => 0,
            B(x) => x,
        }
    }

    fun t04(s: &B<u64>, default: &u64): &u64 {
        match (s) {
            B(0) => default,
            B(x) => x,
        }
    }

    fun t05(s: &mut B<u64>, default: &mut u64): &mut u64 {
        match (s) {
            B(0) => default,
            B(x) => x,
        }
    }

}
