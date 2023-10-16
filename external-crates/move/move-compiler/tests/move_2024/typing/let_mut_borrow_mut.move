module a::m {
    public struct S { f: u64 }
    public fun t(x: u64, s: S) {
        let y = 0;
        let S { f } = s;
        // these three borrows necessiate mut annotations above
        foo(&mut x);
        foo(&mut y);
        foo(&mut f);
    }
    public fun foo<T>(_: &mut T) {}
}
