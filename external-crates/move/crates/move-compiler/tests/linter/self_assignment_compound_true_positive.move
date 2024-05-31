module 0x42::SelfAssignmentCompoundTruePositive {
    fun test_compound_self_assignment(): (u64, u64) {
        let x = 5;
        let y = 10;

        // Compound self-assignments (should not trigger warnings. We have another rules for this)
        x = x + 0;
        y = y * 1;

        (x, y)
    }
}
