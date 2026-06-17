// Test that #[warn(unused)] at the category level works, producing warnings
// for unused variables with a "lint level defined here" note.
module 0x42::m {
    #[warn(unused)]
    fun foo() {
        let x = 0u64;
    }
}
