// Test tracking reference values when multiple levels of references are involved.
module references_deep::m;

public struct SomeStruct has drop {
    struct_field: VecStruct,
}

public struct VecStruct has drop, copy {
    vec_field: vector<u64>,
}

fun bar(vec_ref: &mut vector<u64>): u64 {
    let e = vector::borrow_mut(vec_ref, 0);
    *e = 42;
    vec_ref[0]
}

fun foo(some_struct_ref: &mut SomeStruct): u64 {
    let res = bar(&mut some_struct_ref.struct_field.vec_field);
    res + some_struct_ref.struct_field.vec_field[0]
}

fun some_struct(): SomeStruct {
    SomeStruct {
        struct_field: VecStruct { vec_field: vector::singleton(0) }
    }
}

#[test]
fun test() {
    let mut some_struct = some_struct();
    some_struct.struct_field.vec_field.push_back(7);
    foo(&mut some_struct);
}
