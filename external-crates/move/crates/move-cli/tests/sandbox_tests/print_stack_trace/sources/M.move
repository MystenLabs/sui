module 0x7::M {
    use std::debug;

    public fun sum(n: u64): u64 {
        if (n < 2) {
            debug::print_stack_trace();
            n
        } else {
            n + sum(n - 1)
        }
    }
}
