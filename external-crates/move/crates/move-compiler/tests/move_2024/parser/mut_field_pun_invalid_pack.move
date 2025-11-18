module a::m {
    public struct S { f: u64 }

    public fun foo(): S {
        let _f = 0u64;
        S { mut f }
    }
}
