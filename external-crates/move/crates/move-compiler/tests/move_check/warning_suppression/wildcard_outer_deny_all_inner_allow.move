// Inner #[allow(unused_variable)] should override outer #[deny(all)] for that specific code.
#[deny(all)]
module 0x42::m {
    #[allow(unused_variable)]
    fun allowed(a: u64) {
        let x;
    }

    // No inner override — deny(all) applies.
    fun denied(b: u64) {
        let y;
    }
}
