module a::l {
    public macro fun test($f: ||) {
        $f()
    }
}

module a::m {
    use a::l;

    public struct S { }

    public macro fun test() {
        a::l::test!(|| {
            let s = S { };
            let S { } = s;
        })
    }
}

module a::n {
    use a::m::test;

    public fun t() {
        test!();
    }
}
