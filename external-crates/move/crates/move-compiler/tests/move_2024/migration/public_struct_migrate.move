module a::m {
    struct S { f: u64 }

    struct LongerName {
        f: u64,
        x: S,
    }

    struct Positional(u64, u64, u64)

}
