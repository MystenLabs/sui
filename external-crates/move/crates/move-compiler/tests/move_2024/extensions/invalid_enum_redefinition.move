module a::m {
    public enum E { A() }
}

#[test_only]
extend module a::m {
    public enum E { B() }
}
