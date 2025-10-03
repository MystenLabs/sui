module testing::m {
    public struct S {
        x: u64,
    }

    public fun make_s(x: u64): S {
        S { x }
    }
}

#[test_only]
extend module testing::m {
    // can only be destructured in the same module
    fun destroy_s(s: S) {
        let S { x: _x } = s;
    }

    #[test]
    fun destructure() {
        let s1 = make_s(10);
        // allowed in same module
        let S { x: _x  } = s1;
    }

    #[test]
    fun access_field() {
        let s1 = make_s(10);
        // allowed in same module
        assert!(s1.x == 10);
        destroy_s(s1);
    }

    // ensure unit test poison works
    #[test]
    fun ensure_test_poison() {
        unit_test_poison();
    }
}
