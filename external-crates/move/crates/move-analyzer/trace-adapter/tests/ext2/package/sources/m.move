// Test for assigning a reference to a global location
// in the trace. We have to make sure that when assigning
// into indexed global value (which is a reference), value
// being assinged is of base type rather then a reference
// as this can lead to an infinite cycle when reolving this
// value (if we assign global reference value to the same
// global reference value). In this test this could specifically
// happen in `bar` function when assigning v1 to `some_struct_ref.field`,
module package::global_assign_ref;

public struct SomeStruct has drop, copy {
    field: u64,
}

public struct OuterStruct has drop, copy {
    field: SomeStruct
}

public fun foo (outer_struct_ref: &mut OuterStruct, p: u64): u64 {
    let v1 = outer_struct_ref.field.field;
    bar(&mut outer_struct_ref.field, &v1, p);
    let v2 = outer_struct_ref.field.field + p;
    v2
}

fun bar(some_struct_ref: &mut SomeStruct, f: &u64, p: u64): u64 {
    let v1 = *f + p;
    some_struct_ref.field = v1;
    let v2 = v1 + p;
    some_struct_ref.field = v2;
    v2
}

public fun create_outer_struct(field: u64): OuterStruct {
    OuterStruct { field: SomeStruct { field} }
}

#[test]
fun test() {
    foo(&mut create_outer_struct(42), 7);
}