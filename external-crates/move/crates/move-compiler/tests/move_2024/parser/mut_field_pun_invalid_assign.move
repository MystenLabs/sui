module a::m {
    public struct S { f: u64 }

    public fun foo(s: S) {
        let f = 0;
        S { mut f } = s1;
    }
}
