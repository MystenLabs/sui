// Documentation test: #[allow] on both module and function for the same code.
// The inner (function) scope shadows the outer (module) scope. Both suppress.
#[allow(unused_variable)]
module 0x42::m {
    #[allow(unused_variable)]
    fun foo(a: u64) {
        let x;
    }
}
