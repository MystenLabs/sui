module 0x42::M {
    struct MyResource has copy, drop{
        value: u64,
    }

    public fun test_borrow_deref_ref() {
        let resource = MyResource { value: 10 };

        // Correct usage
        let ref1 = &resource;

        // Simplified borrowing without unnecessary dereference
        let ref2 = &*(&resource);

    }

}