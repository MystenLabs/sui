module a::m {
    public struct S { f: u64 }

    const C: u64 = {
        let mut x = 0;
        x
    };

    public fun foo(mut x: u64, s: S): u64 {
        let mut y = 0;
        let S { mut f } = s;
        bar(&x);
        bar(&y);
        bar(&f);
        x + y + f
    }

    public fun bar(_: &u64) {}
}
