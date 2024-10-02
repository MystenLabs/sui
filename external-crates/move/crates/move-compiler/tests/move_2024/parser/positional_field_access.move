module 0x42::M {
    public struct Foo<T>(T, u64) has drop;

    public struct Bar<T> {
        f: Foo<T>,
    } has drop;

    fun x(y: Foo<u64>): u64 {
        y.0 + y.1
    }

    fun t(y: Bar<u64>): u64 {
        y.f.0 + y.f.1
    }

    fun z(y: Bar<Foo<Bar<u64>>>): u64 {
        y.f.0.0.f.0 + y.f.0.0.f.1 + y.f.1
    }

    fun a(y: Foo<u64>): u64 {
        y.0001 + y.0001
    }

    fun hex(y: Foo<u64>): u64 {
        y.0x0 + y.0xff
    }

    fun underscores(y: Foo<u64>): u64 {
        y.1_0 + y.1_0_0
    }
}
