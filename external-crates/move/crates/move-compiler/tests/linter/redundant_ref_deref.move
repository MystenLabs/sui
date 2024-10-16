module 0x42::TruePositiveCases {
    struct MyResource has copy, drop {
        value: u64,
    }

    public fun case_1() {
        let resource = MyResource { value: 10 };
        let _ref = &*(&resource);  // Redundant borrow-dereference
    }

    public fun case_2() {
        let resource = MyResource { value: 10 };
        let _ref = &mut *(&mut resource);
    }

    public fun case_3() {
        let resource = MyResource { value: 10 };
        let _value = *(&resource.value);
    }
}
