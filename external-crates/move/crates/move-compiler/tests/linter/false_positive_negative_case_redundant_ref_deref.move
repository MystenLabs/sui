module 0x42::FalsePositiveNegativeCases {
    struct MyResource has copy, drop {
        value: u64,
    }

    // False Positive Cases

    public fun case_1() {
        let resource = MyResource { value: 10 };
        let ref1 = &resource;
        let _ref2 = &*ref1;  // Might be intentional for creating a new reference
    }

    public fun case_2<T>(resource: &mut MyResource) {
        let _ref = &mut *resource;  // Might be necessary in generic contexts
    }

    // False Negative Cases

    public fun case_3() {
        let resource = MyResource { value: 10 };
        let _ref = &*(&*(&resource));  // Triple nested borrow-dereference, might be missed
    }

    public fun case_4() {
        let resource = MyResource { value: 10 };
        let ref1 = &resource;
        let _ref2 = &(*ref1);  // Dereference then reference, might be missed
    }

    public fun case_5() {
        let resource = MyResource { value: 10 };
        let _value = *((&resource).value);  // Complex expression, might be missed
    }

    public fun case_6() {
        let resource = MyResource { value: 10 };
        let _ref = &*(&resource.value);  // Complex nested borrow on field, might be missed
    }

    //Field Borrow Cases

    public fun redundant_case() {
        let resource = MyResource { value: 10 };
        let _ref = &*&resource.value;  // Redundant borrow-dereference on field
    }

    public fun nested_case() {
        let resource = MyResource { value: 10 };
        let _ref = &*&(&resource).value;  // Nested redundant borrow-dereference on field
    }
}
