// Documentation test: two separate #[allow] attributes on the same module with overlapping codes.
// Duplicate attributes of the same kind are rejected at parse time.
#[allow(unused_variable)]
#[allow(unused_variable)]
module 0x42::m {
    fun foo(a: u64) {
        let x;
    }
}
