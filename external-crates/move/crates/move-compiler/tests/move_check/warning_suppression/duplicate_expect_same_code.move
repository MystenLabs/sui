// Documentation test: two #[expect] on the same function for the same code.
// Duplicate attributes of the same kind are rejected at parse time.
module 0x42::m {
    #[expect(unused_variable)]
    #[expect(unused_variable)]
    fun foo() {
        let x;
    }
}
