module 0x42::ComplexCases {
    struct MyResource has copy, drop {
        value: u64,
    }

    // Non-Local Borrow Cases

    public fun literal_case() {
        let _ref = &*&0;  // Redundant borrow-dereference on literal
    }

    public fun get_resource(): MyResource {
        MyResource { value: 20 }
    }

    public fun function_call_case() {
        let _ref = &*&get_resource();  // Redundant borrow-dereference on function call result
    }

    //Complex cases

    public fun multiple_field_borrows() {
        let resource = MyResource { value: 10 };
        let _ref = &*&(&*&resource.value);  // Multiple redundant borrows on field
    }

    public fun mixed_borrow_types() {
        let mut resource = MyResource { value: 10 };
        let _ref = &*&mut *&resource;  // Mixed mutable and immutable redundant borrows
    }

    public fun complex_expression() {
        let resource = MyResource { value: 10 };
        let _value = *&(*&resource.value + 1);  // Redundant borrows in complex expression
    }
}
