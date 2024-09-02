module 0x42::true_negative_tests {
    use std::vector;

    public fun access_valid_index() {
        let v = vector[1, 2, 3];
        let _ = vector::borrow(&v, 1); // This should not trigger the lint
    }

    public fun access_first_element() {
        let v = vector[1, 2, 3];
        let _ = vector::borrow(&v, 0); // This should not trigger the lint
    }

    public fun access_after_push() {
        let v = vector[1, 2];
        vector::push_back(&mut v, 3);
        let _ = vector::borrow(&v, 2); // This should not trigger the lint
    }

    public fun access_with_borrow_mut() {
        let v = vector[1, 2, 3];
        let _ = vector::borrow_mut(&mut v, 1); // This should not trigger the lint
    }

    public fun access_in_range_loop() {
        let v = vector[1, 2, 3];
        let i = 0;
        while (i < vector::length(&v)) {
            let _ = vector::borrow(&v, i);
            i = i + 1;
        };
    }

    public fun access_with_length_check() {
        let v = vector[1, 2, 3];
        if (2 < vector::length(&v)) {
            let _ = vector::borrow(&v, 2);
        };
    }

    public fun multiple_valid_accesses() {
        let v = vector[1, 2, 3, 4, 5];
        let _ = vector::borrow(&v, 0);
        let _ = vector::borrow(&v, 2);
        let _ = vector::borrow(&v, 4);
    }
}
