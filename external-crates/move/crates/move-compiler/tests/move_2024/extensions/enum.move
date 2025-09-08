module a::m {
    public enum E has drop {
        A(u64),
        B(bool),
    }
}

#[test_only]
extend module a::m {
    fun g(e: &E): &u64 {
        match (e) {
            E::A(x) => x,
            E::B(_) => abort 1,
        }
    }

    #[test]
    fun test() {
        let e = E::A(42);
        assert!(g(&e) == 42, 0);
    }
}
