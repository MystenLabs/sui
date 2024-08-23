module 0x42::M {
    public struct Foo<T>(T) has drop;
    public struct Bar<T>{ x: T } has drop;

    fun should_pass() {
        let x = Foo(0);
        let y = Foo<u64>(0);
        let Foo<u64>(_) = x;
        Foo<u64>(_) = y;

        let Foo <u64>(_) = Foo(0);
        let Bar <u64>{x: _ } = Bar{ x: 0 };
    }

    fun should_fail() {
        let _ = Foo <u64>(0);
    }
}
