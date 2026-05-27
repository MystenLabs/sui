// module-level suppression
#[allow(lint(unused_return_value))]
module 0x42::m {
    fun pure(x: u64): u64 { x + 1 }

    fun t() { pure(1); }
}

// function-level suppression
module 0x42::n {
    fun pure(x: u64): u64 { x + 1 }

    #[allow(lint(unused_return_value))]
    fun t() { pure(1); }
}
