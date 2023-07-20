module 0x42::unused_fields {

    // there should be unused field warning (no fields)
    native struct NativeStruct;

    // there should be unused field warning (no fields)
    struct EmptyStruct { }

    struct OneUnusedFieldStruct {
        field_used_borrow: u8,
        field_used_borrow_mut: u8,
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

    fun foo(s: &OneUnusedFieldStruct): u8 {
        s.field_used_borrow
    }

    fun bar(s: &mut OneUnusedFieldStruct) {
        s.field_used_borrow_mut = 42;
    }

    fun pack(): AllFieldsUsedPackStruct {
        AllFieldsUsedPackStruct { field1: 42, field2: 7 }
    }

    fun unpack(s: AllFieldsUsedUnpackStruct): (u8, u8) {
        let AllFieldsUsedUnpackStruct { field1, field2 } = s;
        (field1, field2)
    }
}
