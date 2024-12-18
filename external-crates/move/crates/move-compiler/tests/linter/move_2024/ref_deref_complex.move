module 0x42::ComplexCases;

public struct S has copy, drop {
    a: u64,
}

// Non-Local but Simple Borrow Cases

public fun literal_case() {
    let _ref = &*&0;  // Redundant borrow-dereference on literal
}

public fun literal_case_valid() { let _ref = &0; }

public fun get_resource(): S { S { a: 20 } }

public fun function_call_case() {
    let _ref = &*&get_resource();  // Redundant borrow-dereference on function call result
}

public fun function_call_case_valid() { let _ref = &get_resource(); }

//Complex cases

public fun field_borrows() {
    let resource = S { a: 10 };
    let _ref = &*(&*&resource.a);  // Multiple redundant borrows on field
}

public fun field_borrows_valid() {
    let resource = S { a: 10 };
    let _ref = &(copy resource.a);  // Multiple redundant borrows on field
}

public fun mixed_borrow_types() {
    let resource = S { a: 10 };
    let _ref = &*&mut *&resource;  // Mixed mutable and immutable redundant borrows
}

public fun mixed_borrow_types_valid() {
    let resource = S { a: 10 };
    let _ref = &(copy resource);  // Mixed mutable and immutable redundant borrows
}

public fun complex_expression() {
    let resource = S { a: 10 };
    let _a = *&(*&resource.a + 1);  // Redundant borrows in complex expression
}

public fun complex_expression_valid() {
    let resource = S { a: 10 };
    let _a = (copy resource.a) + 1;  // Redundant borrows in complex expression
}
