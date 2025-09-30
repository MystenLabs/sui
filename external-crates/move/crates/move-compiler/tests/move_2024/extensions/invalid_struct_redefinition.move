module a::m {
    public struct S { x: u64 }
}

#[test_only]
extend module a::m {
    public struct S { x: u64 }
}
