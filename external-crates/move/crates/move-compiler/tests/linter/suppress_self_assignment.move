#[allow(lint(self_assignment))]
module a::m {
    fun foo(x: u64): u64 {
        x = x;
        x
    }
}

module a::m2 {
    #[allow(lint(self_assignment))]
    fun foo(x: u64): u64 {
        x = x;
        x
    }
}
