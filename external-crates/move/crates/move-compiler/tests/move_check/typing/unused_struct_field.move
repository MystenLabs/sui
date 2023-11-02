module 0x42::unused_fields {

    // there should be unused field warning (no fields)
    native struct NativeStruct;

    // there should be unused field warning (no fields)
    struct EmptyStruct { }

    struct OneUnusedFieldStruct {
        field_used_borrow: u8,
        field_used_borrow_mut: u8,
        field_used_borrow_var: u8,
        field_unused: u8
    }

    struct AllFieldsUsedPackStruct {
        field1: u8,
        field2: u8,
    }

    struct AllFieldsUsedUnpackStruct {
        field1: u8,
        field2: u8,
    }

    struct AllUnusedFieldsStruct {
        field1: u8,
        field2: u8,
    }

    public fun foo(s: &OneUnusedFieldStruct): u8 {
        s.field_used_borrow
    }

    public fun bar(s: &mut OneUnusedFieldStruct) {
        s.field_used_borrow_mut = 42;
    }

    public fun baz(s: &mut OneUnusedFieldStruct) {
        let v = s;
        v.field_used_borrow_var = 42;
    }

    public fun pack(): AllFieldsUsedPackStruct {
        AllFieldsUsedPackStruct { field1: 42, field2: 7 }
    }

    public fun unpack(s: AllFieldsUsedUnpackStruct): (u8, u8) {
        let AllFieldsUsedUnpackStruct { field1, field2 } = s;
        (field1, field2)
    }
}

// part of the test below is to guard against potential future change to access fields of structs
// from other modules; if this fails, we need to re-think how we implement unused struct field
// warning

module 0x42::private_struct {
    struct S has drop { f: u64 }
}
module 0x42::m {
    struct S has drop { f: u64 }
    public fun flaky(x: 0x42::private_struct::S): u64 { x.f }
}
