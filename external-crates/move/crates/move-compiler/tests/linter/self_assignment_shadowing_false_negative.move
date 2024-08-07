module 0x42::SelfAssignmentShadowingFalseNegative {
    fun test_shadowing_self_assignment(): u64 {
        let x = 5;

        // This is actually a self-assignment of the shadowed 'x', but might be missed
        {
            let x = &mut x;
            *x = *x; // Self-assignment of the outer 'x'
        };

        x
    }
}
