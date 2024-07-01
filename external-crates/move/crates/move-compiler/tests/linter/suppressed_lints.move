module 0x42::M {

    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42; // Should trigger a warning

    #[allow(lint(self_assignment))]
    fun func1(): u64 {
        let x = 5;
        x = x;
        x
    }
}
