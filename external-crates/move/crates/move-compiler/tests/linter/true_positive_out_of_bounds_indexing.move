module 0x42::true_positive_tests {
    use std::vector;

    public fun access_out_of_bounds() {
        let v = vector[1, 2, 3];
        let _ = vector::borrow(&v, 3); // This should trigger the lint
    }

    public fun access_negative_index() {
        let v = vector[1, 2, 3];
        let _ = vector::borrow(&v, 18446744073709551615); // This is -1 as u64, should trigger the lint
    }

    public fun access_empty_vector() {
        let v = vector::empty<u64>();
        let _ = vector::borrow(&v, 0); // This should trigger the lint
    }

    public fun access_after_pop() {
        let v = vector[1, 2, 3];
        vector::pop_back(&mut v);
        let _ = vector::borrow(&v, 2); // This should trigger the lint
    }

    public fun access_large_index() {
        let v = vector[1, 2, 3];
        let _ = vector::borrow(&v, 1000000); // This should trigger the lint
    }

    public fun multiple_out_of_bounds() {
        let v = vector[1, 2, 3];
        let _ = vector::borrow(&v, 3);
        let _ = vector::borrow(&v, 4);
        let _ = vector::borrow(&v, 5);
    }

    public fun out_of_bounds_in_loop() {
        let v = vector[1, 2, 3];
        let i = 0;
        while (i <= 3) { // Note: this loop intentionally goes one step too far
            let _ = vector::borrow(&v, i);
            i = i + 1;
        };
    }
}
