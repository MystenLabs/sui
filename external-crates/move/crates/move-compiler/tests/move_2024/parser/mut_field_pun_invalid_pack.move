module a::m {
    public struct S { f: u64 }

    public fun foo(): S {
        let _f = 0;
        S { mut f }
    }
}
