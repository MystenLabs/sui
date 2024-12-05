// this should warn, but it does not at the current state of optimizations in the compiler

module a::m {
    fun test_equal_operand() {
        let x = 0;
        &x == &0;
    }
}
