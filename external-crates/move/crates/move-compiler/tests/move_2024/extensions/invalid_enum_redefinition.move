module a::m {
    public enum E { A() }
}

#[test_only]
extend a::m {
    public enum E { B() }
}
