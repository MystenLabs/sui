address 0x2 {

module M {
    struct S has drop {}

    fun foo<S: drop>(s1: S, _: S): S {
        (s1: Self::S);
        let _: S = S {}; // TODO error? should this try to construct the generic ?
        bar(s1);
        S {}
    }

    fun bar(_: S) {}
}

}
