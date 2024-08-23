module a::m {
    public struct S<T> { x: T }
    public struct A {}
}

module a::test {
    use a::m::{Self, S, A};

    public fun p(): vector<S

    public fun q(x: S<A
}

