module 0x42::m {
    public struct A<T> { x: T }
    public struct B<T>(T)
    public struct C()

    public enum Either<T,U> {
        Ok(T),
        Err { err: U },
    }

    fun t00(s: A): u64 {
        match (s) {
            S { x: 0 } => 0,
            S { x } => x,
        }
    }

    fun t01(s: &A, default: &u64): &u64 {
        match (s) {
            S { x: 0 } => default,
            S { x } => x,
        }
    }

    fun t02(s: &mut S, default: &mut u64): &mut u64 {
        match (s) {
            S { x: 0 } => default,
            S { x } => x,
        }
    }
}
