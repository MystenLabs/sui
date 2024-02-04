module A::mod3 {
    struct S { f: u64 }

    struct LongerName {
        f: u64,
        x: S,
    }

    struct Positional(u64, u64, u64)

    entry fun t0(x: u64, s: S): u64 {
        let S { f: fin } = s;
        fin = 10;
        x = 20;
        fin + x
    }

}

module A::mod4 {
    public struct S { f: u64 }
    public fun t(x: u64, yip: u64, s: S): u64  {
        let yes = 0;
        let S { f: fin } = s;
        // these four assignments necessiate mut annotations above
        yip = 0;
        x = yes + 1;
        fin = fin + 1;
        yes = x + fin;
        yes
    }
}
