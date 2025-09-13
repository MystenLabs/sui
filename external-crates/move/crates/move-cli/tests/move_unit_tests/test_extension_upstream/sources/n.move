module a::n {
    use testing::m;

    #[test]
    fun destroy() {
        let s1 = m::make_s(10);
        m::destroy_s(s1);
    }
}

#[test_only]
extend module testing::m {
    // can only be destructured in the same module
    public fun destroy_s(s: S) {
        let S { x: _x } = s;
    }
}
