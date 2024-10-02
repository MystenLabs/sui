module Move2024::structs {

    public struct SomeStruct {} has drop, copy;

    public struct Positional(u64, SomeStruct) has drop, copy;

    public fun foo(positional: Positional): (u64, SomeStruct) {
        (positional.0, positional.1)
    }

    public struct Named has drop, copy {
        some_field: u64,
        another_field: SomeStruct,
    }

    public fun pack_named(val1: u64, another_field: SomeStruct): Named {
        Named {
            some_field: val1,
            another_field,
        }
    }

    public fun pack_positional(val1: u64, val2: SomeStruct): Positional {
        Positional(val1, val2)
    }

    public fun unpack_named(named: Named): (u64, SomeStruct) {
        let Named {
            some_field: val1,
            another_field,
        } = named;
        (val1, another_field)
    }

    public fun unpack_positional(positional: Positional): (u64, SomeStruct) {
        let Positional(val1, val2) = positional;
        (val1, val2)
    }

    public fun borrow_named(named: Named): u64 {
        named.some_field
    }

    public fun borrow_positional(positional: Positional): u64 {
        positional.0
    }

}
