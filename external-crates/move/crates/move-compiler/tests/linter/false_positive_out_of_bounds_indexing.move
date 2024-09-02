module 0x42::false_positive_tests {
    use std::vector;

    public fun access_last_element() {
        let v = vector[1, 2, 3];
        let last_index = vector::length(&v) - 1;
        let _ = vector::borrow(&v, last_index); // This might trigger the lint, but it's actually safe
    }

    public fun access_with_length_check() {
        let v = vector[1, 2, 3];
        let i = 2;
        if (i < vector::length(&v)) {
            let _ = vector::borrow(&v, i); // This is safe but might trigger the lint
        };
    }

    public fun access_after_push() {
        let v = vector[1, 2];
        vector::push_back(&mut v, 3);
        let _ = vector::borrow(&v, 2); // This is safe but might trigger the lint
    }

    public fun access_with_complex_calculation() {
        let v = vector[1, 2, 3, 4, 5];
        let mid = vector::length(&v) / 2;
        let _ = vector::borrow(&v, mid); // This is safe but might trigger the lint
    }

    public fun access_with_modulo() {
        let v = vector[1, 2, 3];
        let i = 5;
        let _ = vector::borrow(&v, i % vector::length(&v)); // This is safe but might trigger the lint
    }

    public fun access_in_loop_with_check() {
        let v = vector[1, 2, 3];
        let i = 0;
        while (i < vector::length(&v)) {
            let _ = vector::borrow(&v, i); // This is safe but might trigger the lint in a loop
            i = i + 1;
        };
    }
}
