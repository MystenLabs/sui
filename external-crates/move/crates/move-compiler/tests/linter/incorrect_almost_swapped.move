module 0x42::M {

    fun func1(): (u64, u64) {
        let foo = 1;
        let bar = 2;

        foo = bar;
        bar = foo;

        (foo, bar)
    }
}
