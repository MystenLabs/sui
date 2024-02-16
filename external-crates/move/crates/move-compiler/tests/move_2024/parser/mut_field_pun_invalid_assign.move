module a::m {
    public struct S has drop { f: u64 }

    public fun foo(_s: S) {
        let f = 0;
        S { mut f } = s1;
    }
}
