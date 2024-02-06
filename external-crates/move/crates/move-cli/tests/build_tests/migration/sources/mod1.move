module A::mod1 {
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
