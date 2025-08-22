module structs::structs;

public struct Foo {
    val: u64,
}

public struct Bar {
    val: u64,
    other: u64,
}

public fun unpack(foo: Foo) : u64 {
    let Foo { val } = foo;
    val
}

public fun unpack_bar(bar: Bar) : (u64, u64) {
    let Bar { val, other } = bar;
    (val, other)
}