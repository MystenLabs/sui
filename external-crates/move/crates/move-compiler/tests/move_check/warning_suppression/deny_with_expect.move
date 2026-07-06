// Test interaction: #[deny] on a module, #[expect] on a function.
// The expect should suppress the denied warning and be fulfilled.
#[deny(unused_variable)]
module 0x42::m {
    #[expect(unused_variable)]
    fun expected(a: u64) {
        let x;
    }

    // No expect here — deny should apply and upgrade to error.
    fun denied(a: u64) {
        let y;
    }
}
