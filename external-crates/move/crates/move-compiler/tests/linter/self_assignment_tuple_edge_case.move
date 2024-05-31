module 0x42::SelfAssignmentTupleEdgeCase {
    fun test_tuple_assignment(): (u64, u64) {
        let (x, y) = (5, 10);

        // This is not a self-assignment, but involves reassigning variables (should not trigger warning)
        (x, y) = (y, x);

        // This is a self-assignment and should trigger a warning
        (x, y) = (x, y);

        (x, y)
    }
}
