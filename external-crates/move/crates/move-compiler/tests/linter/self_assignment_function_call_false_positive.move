module 0x42::SelfAssignmentFunctionCallFalsePositive {
    fun get_value(x: u64): u64 { x }

    fun test_function_call_assignment(): u64 {
        let x = 5;

        // This looks like a self-assignment but isn't (should not trigger warning)
        x = get_value(x);

        x
    }
}
