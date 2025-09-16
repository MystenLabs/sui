module a::m {
    public struct S has drop { x: u64 }

    fun get_x(s: &S): &u64 { &s.x }
}

#[test_only]
extend module a::m {
    use fun g as S.get_x;

    fun g(s: &S): &u64 { s.get_x() }

    #[test]
    fun test() {
        let s = S { x: 24 };
        assert!(g(&s) == 24, 0);
    }
}
