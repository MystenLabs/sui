#[allow(lint(equal_operands))]
module a::suppress_equal_operand {
    fun suppressed_function<T: copy + drop>(x: T): bool {
        x == x
    }
}

module b::suppress_equal_operand {
    #[allow(lint(equal_operands))]
    fun suppressed_function<T: copy + drop>(x: T): bool {
        x == x
    }
}
