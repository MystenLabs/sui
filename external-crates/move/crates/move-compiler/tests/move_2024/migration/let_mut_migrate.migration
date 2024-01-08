module a::m {
    public struct S { f: u64 }
    public fun t(x: u64, s: S): u64  {
        let y = 0;
        let S { f } = s;
        // these three assignments necessiate mut annotations above
        x = y + 1;
        f = f + 1;
        y = x + f;
        y
    }
}
