module 0x2::Test {
    fun do_nothing(): bool {
        false
    }

    spec do_nothing {
        ensures result == multi_foo(true, false);
        ensures result == single_foo(false);
    }

    spec fun multi_foo(x: bool, y: bool): bool {
        let a = x;
        let b = y;
        a && b
    }

    spec fun single_foo(x: bool): bool {
        let a = x;
        a
    }

}
