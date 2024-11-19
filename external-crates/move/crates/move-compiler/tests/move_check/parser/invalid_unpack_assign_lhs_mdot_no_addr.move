module 0x42::M {
    fun foo() {
        let _f = 0;
        ERROR
        M { f } = 0;

        let _f = 0;
        ERROR
        {
            f
        } = 0;

        let _f = 0;
        foo().M { f } = 0;
    }
}
