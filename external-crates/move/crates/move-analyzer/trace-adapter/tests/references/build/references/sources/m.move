// Test tracking reference values.
module references::m;

public struct SomeStruct has drop {
    struct_field: SimpleStruct,
    simple_field: u64,
    vec_simple_field: vector<u64>,
}

public struct SimpleStruct has drop, copy {
    field: u64,
}

fun foo(
    some_struct_ref: &mut SomeStruct,
    vec_ref: &mut vector<u64>,
    num_ref: &u64,
): u64 {
    some_struct_ref.struct_field.field = 42;
    some_struct_ref.simple_field = *num_ref;

    let e1 = vector::borrow_mut(&mut some_struct_ref.vec_simple_field, 0);
    *e1 = 42;

    let e2 = vector::borrow_mut(vec_ref, 0);
    *e2 = 42;
    *num_ref + some_struct_ref.simple_field + vec_ref[0]
}

fun some_struct(): SomeStruct {
    SomeStruct {
        struct_field: SimpleStruct { field: 0 },
        simple_field: 0,
        vec_simple_field: vector::singleton(0),
    }
}

#[test]
fun test() {
    let mut some_struct = some_struct();
    let mut vec = vector::singleton(0);
    vector::push_back(&mut vec, 7);
    let num = 42;
    foo(&mut some_struct, &mut vec, &num);
}
