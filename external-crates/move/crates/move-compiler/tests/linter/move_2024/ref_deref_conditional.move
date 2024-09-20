module 0x42::ConstrainedRefDerefCases;

struct MyResource has copy, drop {
    value: u64,
}

// Case 1: Should be flagged - simple &*& pattern
public fun should_flag_simple() {
    let resource = MyResource { value: 10 };
    let _ref = &*&resource;  // Should be flagged
}

// Case 2: Should be flagged - &mut *& pattern
public fun should_flag_mut() {
    let resource = MyResource { value: 10 };
    let _ref = &mut *&resource;  // Should be flagged
}

// Case 3: Should be flagged - &*& pattern with field access
public fun should_flag_field() {
    let mut resource = MyResource { value: 10 };
    let _ref = &*&resource.value;  // Should be flagged
    resource.value = 10;
}

// Case 3: Should not be flagged - &* pattern without extra &
public fun should_not_flag_modified() {
    let resource = MyResource { value: 10 };
    let ref1 = &resource;
    resource.value = 20;  // Modifying the resource
    let _ref2 = &*ref1;  // No flag -- ref was made elsewhere
}

// Case 5: Should be flagged - nested &*& pattern
public fun should_flag_nested() {
    let resource = MyResource { value: 10 };
    let _ref = &*(&*&resource);  // Should be flagged
}

// Case 6: Should not be flagged - &* pattern without extra &
public fun should_not_flag_deref_only() {
    let resource = MyResource { value: 10 };
    let ref1 = &resource;
    let _ref2 = &*ref1;  // Should not be flagged
}

// Case 7: Should be flagged - *& pattern without leading &
public fun should_flag_copy() {
    let resource = MyResource { value: 10 };
    let _copy = *&resource;  // Should be flagged, making a copy
}

// Case 8: Should be flagged for removal - &*& pattern with function call
public fun get_resource(): MyResource {
    MyResource { value: 20 }
}

public fun should_flag_function_call() {
    let _ref = &*&get_resource();  // Should be flagged
}

// Case 9: Should be flagged for removal - &*& pattern with constant value
public fun should_flag_value() {
    let _ref = &*&0;  // Should be flagged
}

// Case 10: Should be flagged - &*& pattern but path is mutated in a loop
public fun should_flag_loop_mutation() {
    let mut resource = MyResource { value: 10 };
    let mut i = 0;
    while (i < 5) {
        let _ref = &*&resource;  // Should be flagged regardless
        resource.value = resource.value + 1;
        i = i + 1;
    }
}

const E: u64 = 0;

// Case 11: Should be flagged -- constant
#[allow(implicit_const_copy)]
public fun should_flag_constant() {
    let _ref = &*&E;  // Should be flagged
}

// Case 12: Should be flagged -- vector
public fun should_flag_vector() {
    let _ref = &*&vector[1,2,3];  // Should be flagged
}
