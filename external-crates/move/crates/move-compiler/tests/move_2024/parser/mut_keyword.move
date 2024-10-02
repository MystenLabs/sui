module a::m {
    public struct S {
        f: u64,
    }
    public fun foo(mut: &mut u64): &mut u64 {
        mut
    }
}
