module 0x42::suppress_case_tests {
    use std::vector;

    #[allow(lint(out_of_bounds_indexing))]
    public fun suppress_simple_case() {
        let v = vector[1, 2, 3];
        let _ = vector::borrow(&v, 3); // This would normally trigger the lint, but it's suppressed
    }

    #[allow(lint(out_of_bounds_indexing))]
    public fun suppress_multiple_accesses() {
        let v = vector[1, 2, 3];
        let _ = vector::borrow(&v, 3);
        let _ = vector::borrow(&v, 4);
        let _ = vector::borrow(&v, 5);
    }

    #[allow(lint(out_of_bounds_indexing))]
    public fun suppress_in_loop() {
        let v = vector[1, 2, 3];
        let i = 0;
        while (i < 5) { // This loop intentionally goes out of bounds
            let _ = vector::borrow(&v, i);
            i = i + 1;
        };
    }

    #[allow(lint(out_of_bounds_indexing))]
    public fun suppress_with_calculation() {
        let v = vector[1, 2, 3];
        let i = vector::length(&v) * 2;
        let _ = vector::borrow(&v, i);
    }

    #[allow(lint(out_of_bounds_indexing))]
    public fun suppress_empty_vector_access() {
        let v = vector::empty<u64>();
        let _ = vector::borrow(&v, 0);
    }

}
