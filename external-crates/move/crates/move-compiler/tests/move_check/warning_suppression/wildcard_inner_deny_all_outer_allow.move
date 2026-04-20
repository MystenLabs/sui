// Inner #[deny(all)] should override outer #[allow(unused_variable)].
#[allow(unused_variable)]
module 0x42::m {
    #[deny(all)]
    fun denied(a: u64) {
        let x;
    }

    // Inherits outer allow — no warning.
    fun allowed(b: u64) {
        let y;
    }
}
