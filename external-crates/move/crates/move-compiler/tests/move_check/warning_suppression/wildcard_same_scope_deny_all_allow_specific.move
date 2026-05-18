// Same scope: specific #[allow(unused_variable)] should beat wildcard #[deny(all)].
#[deny(all)]
#[allow(unused_variable)]
module 0x42::m {
    fun foo(a: u64) {
        let x;
    }
}
