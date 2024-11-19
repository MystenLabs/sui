// structs conflict since types can be used to start an access chain
module a::S {
    public struct S() has copy, drop;
}

#[allow(unused)]
module a::m {
    use a::S::{Self, S};
}

module a::n {
    use a::S;

    public struct S() has copy, drop; // ERROR
}
