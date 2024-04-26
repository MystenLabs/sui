module 0x42::M {
    struct S { f: u64 }
    fun foo() {
        let _s = S { false };
        let _s = S { 0 };
    }
}
