#[allow(lint(always_equal_operands))]
module a::m {
    fun t(x: u64): bool { x == x }
}

module a::n {
    #[allow(lint(always_equal_operands))]
    fun t(x: u64): bool { x == x }
}
