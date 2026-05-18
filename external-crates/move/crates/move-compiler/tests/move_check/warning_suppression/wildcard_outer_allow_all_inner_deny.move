// Inner #[deny(unused_variable)] should override outer #[allow(all)].
#[allow(all)]
module 0x42::m {
    #[deny(unused_variable)]
    fun denied(a: u64) {
        let x;
    }

    // Inherits outer allow(all) — no warning.
    fun allowed(b: u64) {
        let y;
    }
}
