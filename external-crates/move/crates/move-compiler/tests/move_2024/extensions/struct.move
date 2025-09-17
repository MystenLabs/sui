module a::m {
    public struct S has drop { x: u64 }
}

#[test_only]
extend module a::m {
    fun g(s: &S): &u64 {
        let S { x } = s;
        x
    }

    #[test]
    fun test() {
        let s = S { x: 7 };
        assert!(g(&s) == 7, 0);
    }
}
