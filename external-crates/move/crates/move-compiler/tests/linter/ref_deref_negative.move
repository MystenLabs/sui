module 0x42::TrueNegativeCases {
    struct MyResource has copy, drop {
        value: u64,
    }

    public fun case_1() {
        let resource = MyResource { value: 10 };
        let _ref = &resource;  // Direct borrow, no redundancy
    }

    public fun case_2() {
        let resource = MyResource { value: 10 };
        let ref = &resource;
        let _value = ref.value;  // Accessing field through reference, no redundancy
    }

    public fun case_3() {
        let resource = MyResource { value: 10 };
        let _copy = resource;  // Creating a copy, no borrow involved
    }
}
