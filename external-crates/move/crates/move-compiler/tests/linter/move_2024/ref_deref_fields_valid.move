module 0x42::complex_ref_deref_valid;

public struct MyResource has copy, drop {
    value: u64,
}

// Flagged Cases

public fun case_4() {
    let resource = MyResource { value: 10 };
    let _ref = & copy resource;
}

public struct S<T: copy + drop> has drop {
    value: T,
}

public fun case_5<T: copy + drop>(s: S<T>) {
    let _value: T = s.value;  // Complex field expression
}

public fun case_5_b<T: copy + drop>(s: S<T>) {
    let _value: T = s.value; // Complex field expression
}

public fun case_5_c<T: copy + drop>(s: &S<T>) {
    let _value: T  = s.value; // Complex field expression -- bad copy
}

public fun case_5_d<T: copy + drop>(s: &mut S<T>) {
    let _value: T  = s.value; // Complex field expression -- bad copy
}

public fun case_6() {
    let resource = MyResource { value: 10 };
    let _ref = &resource.value;  // Complex nested borrow on field
}

//Field Borrow Cases

public fun redundant_case() {
    let resource = MyResource { value: 10 };
    let _ref = &resource.value;  // Direct, redundant borrow-dereference on field
}

public fun nested_case() {
    let resource = MyResource { value: 10 };
    let _ref = &(&resource).value;  // Nested redundant borrow-dereference on field
}
