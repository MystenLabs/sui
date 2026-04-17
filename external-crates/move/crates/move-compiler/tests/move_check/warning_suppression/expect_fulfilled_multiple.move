// Test that #[expect] with multiple specific filters: each is tracked independently.
// Both unused_variable and dead_code fire, so both expectations are fulfilled.
module 0x42::m {
    #[expect(unused_variable, dead_code)]
    fun both_fire() {
        let x = 0u64;
        loop {};
        abort 0
    }
}
