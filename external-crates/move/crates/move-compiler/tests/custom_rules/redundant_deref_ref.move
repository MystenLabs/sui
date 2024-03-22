module 0x42::M {
    struct MyResource has copy, drop{
        value: u64,
    }

    public fun test_borrow_deref_ref() {
        let resource = MyResource { value: 10 };

        // Correct usage
        let ref1 = &resource;
        let value1 = *ref1;

        // Unnecessary dereference and then borrow pattern
        let ref2 = &*(&resource);
        let value2 = *ref2;

        let resource_ref = &resource;
        let resource_ref_mut = &mut resource;



    }

}