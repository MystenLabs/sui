//# publish
module 0x42::test_case {
    struct Foo has drop {}

    fun bar(_foo: &mut Foo) {
        /* ... */
    }

    fun remove_tx(foo: &mut Foo) {
        let i = 0;
        while (i < 64) {
            i = i + 1;

            bar(foo);
            if (i < 30) {
                continue
            };

            break
        }
    }
}
