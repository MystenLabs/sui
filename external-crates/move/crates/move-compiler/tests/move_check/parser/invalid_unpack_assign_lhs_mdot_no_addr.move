module 0x42::M {
    fun foo() {
        let _f = 0;
        false::M { f } = 0;

        let _f = 0;
        0::M { f } = 0;

        let _f = 0;
        foo().M { f } = 0;
    }

}
