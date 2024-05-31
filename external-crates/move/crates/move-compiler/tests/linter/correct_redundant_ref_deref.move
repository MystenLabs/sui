module 0x42::M {
    struct MyResource has copy, drop{
        value: u64,
    }

    public fun test_borrow_deref_ref() {
        let resource = MyResource { value: 10 };
        let _ref1 = &resource;
    }
}
