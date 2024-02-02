module A::mod2 {
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

    public fun t2(): u64 {
        let mut x = 5; let mut y = 10;
        x = x + 1;
        y = x + 1;
        x + y
    }
}
