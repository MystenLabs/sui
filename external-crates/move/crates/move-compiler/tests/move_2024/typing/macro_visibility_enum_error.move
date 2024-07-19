module a::m {

    public enum E {
        V()
    }

    public macro fun test() {
        let e = E::V();
        match (e) {
            E::V() => (),
        };
    }
}

module a::n {
    use a::m::test;

    public fun t() {
        test!();
    }
}
