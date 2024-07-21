module 0x42::M {

    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42; // Should trigger a warning

    #[allow(lint(swap_sequence))]
    fun func1(foo: u64, bar: u64): (u64, u64) {
        foo = bar;
        bar = foo;

        (foo, bar)
    }
}
