module 0x42::SelfAssignmentComplexTruePositive {
    struct S { x: u64 }

    fun test_complex_self_assignment(s: &mut S): (u64, u64) {
        let x = 5;
        let y = &mut x;

        // Self-assignment in struct field (should trigger warning)
        s.x = s.x;

        (x, x)
    }
}
