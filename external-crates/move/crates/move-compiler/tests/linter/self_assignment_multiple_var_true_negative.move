module 0x42::SelfAssignmentMultipleVarTrueNegative {
    fun test_multiple_var_no_self_assignment(): (u64, u64, u64) {
        let (x, y, z) = (1, 2, 3);

        // Multiple assignments, but not self-assignments (should not trigger warnings)
        x = y;
        y = z;
        z = x + y;

        (x, y, z)
    }
}
