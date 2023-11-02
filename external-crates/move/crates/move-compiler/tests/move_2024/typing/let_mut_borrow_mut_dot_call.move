module a::m {
    public struct S has drop { f: u64 }
    public struct R has drop { s: S }
    public fun t(x: S, r: R) {
        let y = S { f: 0 };
        let R { s } = r;
        // these three borrows necessiate mut annotations above
        x.foo();
        y.foo();
        s.foo();
    }
    public fun foo(_: &mut S) {}
}
