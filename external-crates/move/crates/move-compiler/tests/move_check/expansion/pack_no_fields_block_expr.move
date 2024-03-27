module 0x42::M {
    struct S {}
    fun foo() {
        let _s = S { let x = 0; x };
        let _s = S { let y = 0; let z = 0; x + foo() };
    }
}
