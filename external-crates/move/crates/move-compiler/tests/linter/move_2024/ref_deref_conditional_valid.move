module 0x42::ConstrainedRefDerefCases;

public struct MyResource has copy, drop {
    value: u64,
}

// Case 1
public fun should_not_flag_simple() {
    let resource = MyResource { value: 10 };
    let _ref = & resource;
}

// Case 2
public fun should_not_flag_mut() {
    let resource = MyResource { value: 10 };
    let _ref = &mut copy resource;
}

// Case 3
public fun should_not_flag_field() {
    let mut resource = MyResource { value: 10 };
    let _ref = &resource.value;
    resource.value = 10;
}

// Case 4 -- invalid control flow
public fun should_not_flag_modified() {
    let mut resource = MyResource { value: 10 };
    let ref1 = &copy resource;
    resource.value = 20;
    let _ref2 = &*ref1;  // No flag -- ref was made elsewhere
}

// Case 5
public fun should_not_flag_nested() {
    let resource = MyResource { value: 10 };
    let _ref = &copy resource;
}

// Case 6
public fun should_not_flag_deref_only() {
    let resource = MyResource { value: 10 };
    let ref1 = &resource;
    let _ref2 = &*ref1;  // Should not be flagged
}

// Case 7
public fun should_not_flag_copy() {
    let resource = MyResource { value: 10 };
    let _copy = copy resource;
}

// Case 8
public fun get_resource(): MyResource {
    MyResource { value: 20 }
}

// Case 9
public fun should_not_flag_value() {
    let _ref = &0;
}

// Case 10
public fun should_not_flag_loop_mutation() {
    let mut resource = MyResource { value: 10 };
    let mut i = 0;
    while (i < 5) {
        let _ref = &copy resource;  // Should be flagged regardless
        resource.value = resource.value + 1;
        i = i + 1;
    }
}

const E: u64 = 0;

// Case 11
#[allow(implicit_const_copy)]
public fun should_flag_constant() {
    let _ref = &E;  // Should be flagged
}

// Case 12
public fun should_not_flag_vector() {
    let _ref = &vector[1,2,3];  // Should be flagged
}
