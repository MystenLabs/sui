module a::m {
    public struct S has drop { x: u64 }

    fun get_x(s: &S): &u64 { &s.x }
}

#[test_only]
extend module a::m {
    use fun get_x as S.to_x;

    #[test]
    fun test() {
        let s = S { x: 24 };
        assert!(s.to_x() == 24, 0);
    }
}
