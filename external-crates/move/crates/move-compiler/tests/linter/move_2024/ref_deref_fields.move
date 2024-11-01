module 0x42::complex_ref_deref;

public struct MyResource has copy, drop {
    value: u64,
}

// Ignored Cases

public fun case_1() {
    let resource = MyResource { value: 10 };
    let ref1 = &resource;
    let _ref2 = &*ref1;  // Might be intentional for creating a new reference
}

public fun case_2<T>(resource: &mut MyResource) {
    let _ref = &mut *resource;  // Might be necessary in generic contexts
}

public fun case_3() {
    let resource = MyResource { value: 10 };
    let ref1 = &resource;
    let _ref2 = &(*ref1);  // Dereference then reference -- c'est la vie
}

public fun case_5<T: copy + drop>(s: S<T>) {
    let _value: T = *&((&s).value);  // Complex field expression
                                     // Could be removed in favor of the implicit copy, but path
                                     // processing makes it unclear what to do after typing rewrites
                                     // it. See note in lint analysis.
}

// Flagged Cases

public fun case_4() {
    let resource = MyResource { value: 10 };
    let _ref = &*(&*(&resource));  // Triple nested borrow-dereference, might be missed
}

public struct S<T: copy + drop> has drop {
    value: T,
}

public fun case_5_b<T: copy + drop>(s: S<T>) {
    let _value: T = *&(copy s.value); // Complex field expression
                                      // Could be removed in favor of the implicit copy
}

public fun case_5_c<T: copy + drop>(s: &S<T>) {
    let _value: T  = copy s.value; // Complex field expression -- bad copy
}

public fun case_5_d<T: copy + drop>(s: &mut S<T>) {
    let _value: T  = copy s.value; // Complex field expression -- bad copy
}

public fun case_6() {
    let resource = MyResource { value: 10 };
    let _ref = &*(&resource.value);  // Complex nested borrow on field
}

//Field Borrow Cases

public fun redundant_case() {
    let resource = MyResource { value: 10 };
    let _ref = &*&resource.value;  // Direct, redundant borrow-dereference on field
}

public fun nested_case() {
    let resource = MyResource { value: 10 };
    let _ref = &*&(&resource).value;  // Nested redundant borrow-dereference on field
}
