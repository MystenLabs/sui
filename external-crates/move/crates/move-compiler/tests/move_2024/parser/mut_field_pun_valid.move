module a::m {
    public struct S { f: u64 }

    public fun foo(s1: S, s2: &mut S, s3: &S) {
        let mut x = 0;
        let S { mut f } = s1;
        f = f + 1;
        f;

        let S { mut f } = s2;
        f;
        f = &mut x;
        f;

        let S { mut f } = s3;
        f;
        f = &x;
        f;
    }
}
