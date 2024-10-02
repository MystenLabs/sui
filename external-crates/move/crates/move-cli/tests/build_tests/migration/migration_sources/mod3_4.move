module A::mod3 {
    public struct S { f: u64 }

    public struct LongerName {
        f: u64,
        x: S,
    }

    public struct Positional(u64, u64, u64)

    entry fun t0(mut x: u64, s: S): u64 {
        let S { f: mut fin } = s;
        fin = 10;
        x = 20;
        fin + x
    }

}

module A::mod4 {
    public struct S { f: u64 }
    public fun t(mut x: u64, mut yip: u64, s: S): u64  {
        let mut yes = 0;
        let S { f: mut fin } = s;
        // these four assignments necessiate mut annotations above
        yip = 0;
        x = yes + 1;
        fin = fin + 1;
        yes = x + fin;
        yes
    }
}
