module A::mod0 {
    struct S { f: u64 }

    struct LongerName {
        f: u64,
        x: S,
    }

    struct Positional(u64, u64, u64)

    entry fun t0(x: u64, s: Positional): u64 {
        let S { f: fin } = s;
        fin = 10;
        x = 20;
        fin + x
    }

}
