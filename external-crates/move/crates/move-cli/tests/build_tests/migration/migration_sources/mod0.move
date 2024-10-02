module A::mod0 {
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
