// Test tracking values of compound type variables
// (structs, enums, vectors).
module compound::m;

public enum SomeEnum<T> has drop {
    PositionalVariant(u64, T),
    NamedVariant { field1: u64, field2: u64 },
}

public struct SomeStruct<T, S> has drop {
    simple_field: u64,
    enum_field: SomeEnum<S>,
    another_enum_field: SomeEnum<S>,
    vec_simple_field: vector<u64>,
    vec_struct_field: vector<T>,
}

public struct SimpleStruct<T> has drop, copy {
    field: T,
}

fun foo(mut some_struct: SomeStruct<SimpleStruct<u64>, u64>, p: u64): SomeStruct<SimpleStruct<u64>, u64> {
    let pos_variant = SomeEnum::PositionalVariant(p, p);
    let named_variant = SomeEnum::NamedVariant {
        field1: p,
        field2: p,
    };
    let v = vector::singleton(p);
    let v_struct = vector::singleton(SimpleStruct { field: p });

    some_struct.simple_field = p;
    some_struct.enum_field = pos_variant;
    some_struct.another_enum_field = named_variant;
    some_struct.vec_simple_field = v;
    some_struct.vec_struct_field = v_struct;

    some_struct
}

fun some_struct(): SomeStruct<SimpleStruct<u64>, u64> {
    SomeStruct {
        simple_field: 0,
        enum_field: SomeEnum::PositionalVariant(0, 0),
        another_enum_field: SomeEnum::PositionalVariant(0, 0),
        vec_simple_field: vector::singleton(0),
        vec_struct_field: vector::singleton(SimpleStruct { field: 0 }),
    }
}

#[test]
fun test() {
    let some_struct = some_struct();
    foo(some_struct, 42);
}
