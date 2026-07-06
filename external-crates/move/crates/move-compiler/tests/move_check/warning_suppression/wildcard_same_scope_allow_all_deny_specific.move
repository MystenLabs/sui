// Same scope: specific #[deny(unused_variable)] should beat wildcard #[allow(all)].
#[allow(all)]
#[deny(unused_variable)]
module 0x42::m {
    fun foo(a: u64) {
        let x;
    }
}
