module 0x42::M {
    struct MyResource has copy, drop {
        value: u64,
    }

    // True Positive Cases

    public fun true_positive_1() {
        let resource = MyResource { value: 10 };
        let _ref = &*(&resource);  // Redundant borrow-dereference
    }

    public fun true_positive_2() {
        let resource = MyResource { value: 10 };
        let _ref = &mut *(&mut resource);  // Redundant mutable borrow-dereference
    }

    public fun true_positive_3() {
        let resource = MyResource { value: 10 };
        let _value = *(&resource.value);  // Redundant dereference of field borrow
    }

    // True Negative Cases

    public fun true_negative_1() {
        let resource = MyResource { value: 10 };
        let _ref = &resource;  // Direct borrow, no redundancy
    }

    public fun true_negative_2() {
        let resource = MyResource { value: 10 };
        let ref = &resource;
        let _value = ref.value;  // Accessing field through reference, no redundancy
    }

    public fun true_negative_3() {
        let resource = MyResource { value: 10 };
        let _copy = resource;  // Creating a copy, no borrow involved
    }

    // False Positive Cases

    public fun false_positive_2() {
        let resource = MyResource { value: 10 };
        let ref1 = &resource;
        let _ref2 = &*ref1;  // Might be intentional for creating a new reference
    }

    public fun false_positive_3<T>(resource: &mut MyResource) {
        let _ref = &mut *resource;  // Might be necessary in generic contexts
    }

    // False Negative Cases

    public fun false_negative_1() {
        let resource = MyResource { value: 10 };
        let _ref = &*(&*(&resource));  // Triple nested borrow-dereference, might be missed
    }

    public fun false_negative_2() {
        let resource = MyResource { value: 10 };
        let ref1 = &resource;
        let _ref2 = &(*ref1);  // Dereference then reference, might be missed
    }

    public fun false_negative_3() {
        let resource = MyResource { value: 10 };
        let _value = *((&resource).value);  // Complex expression, might be missed
    }

    // New cases for field borrows

    public fun field_borrow_redundant() {
        let resource = MyResource { value: 10 };
        let _ref = &*&resource.value;  // Redundant borrow-dereference on field
    }

    public fun field_borrow_nested() {
        let resource = MyResource { value: 10 };
        let _ref = &*&(&resource).value;  // Nested redundant borrow-dereference on field
    }

    // New cases for non-local borrows

    public fun non_local_borrow_literal() {
        let _ref = &*&0;  // Redundant borrow-dereference on literal
    }

    public fun get_resource(): MyResource {
        MyResource { value: 20 }
    }

    public fun non_local_borrow_function_call() {
        let _ref = &*&get_resource();  // Redundant borrow-dereference on function call result
    }

    // Helper method for the above test cases
    public fun do_something(self: &MyResource) {
        // Dummy implementation
        let _ = self.value;
    }

    // Additional test cases to cover more scenarios

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

    // False negative cases for the new scenarios

    public fun false_negative_complex_field_borrow() {
        let resource = MyResource { value: 10 };
        let _ref = &*(&resource.value);  // Complex nested borrow on field, might be missed
    }

    // Suppress Cases

    #[allow(lint(redundant_ref_deref))]
    public fun suppress_case_1() {
        let resource = MyResource { value: 10 };
        let _ref = &*(&resource);  // Suppressed warning
    }

    #[allow(lint(redundant_ref_deref))]
    public fun suppress_case_2() {
        let resource = MyResource { value: 10 };
        let _value = *(&resource.value);  // Suppressed warning
    }
}
