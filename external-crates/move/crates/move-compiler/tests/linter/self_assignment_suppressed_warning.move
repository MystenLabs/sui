module 0x42::SelfAssignmentSuppressedWarning {
    // Test suppression at the function level
    #[allow(lint(constant_naming))]
    fun suppressed_function(): u64 {
        let a = 20;
        a = a; // This should not trigger a warning
        a
    }
}
