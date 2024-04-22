module 0x42::M {
    struct S { f: u64 }
    fun foo() {
        let _f: u64;
        { f } = S { f: 0 };

        S f = S { f: 0 };
    }
}
