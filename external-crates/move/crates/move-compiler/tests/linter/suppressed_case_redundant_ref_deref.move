module 0x42::SuppressCases {
    struct MyResource has copy, drop {
        value: u64,
    }

    #[allow(lint(redundant_ref_deref))]
    public fun case_1() {
        let resource = MyResource { value: 10 };
        let _ref = &*(&resource);  // Suppressed warning
    }

    #[allow(lint(redundant_ref_deref))]
    public fun case_2() {
        let resource = MyResource { value: 10 };
        let _value = *(&resource.value);  // Suppressed warning
    }
}
