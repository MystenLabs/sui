// Inner #[allow(all)] should override outer #[deny(unused_variable)].
#[deny(unused_variable)]
module 0x42::m {
    #[allow(all)]
    fun allowed(a: u64) {
        let x;
    }

    // Inherits outer deny — errors.
    fun denied(b: u64) {
        let y;
    }
}
