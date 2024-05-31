module 0x42::M {

    fun func1(): (u64, u64) {
        let foo = 1;
        let bar = 2;
        let temp;
        // Proper swap using a temporary variable (should not trigger the linter)
        temp = foo;
        foo = bar;
        bar = temp;

        (foo, bar)
    }
}
