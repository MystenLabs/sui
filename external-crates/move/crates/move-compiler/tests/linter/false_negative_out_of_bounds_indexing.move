module 0x42::false_negative_tests {
    use std::vector;

    public fun access_with_parameter(i: u64) {
        let v = vector[1, 2, 3];
        let _ = vector::borrow(&v, i); // This might not trigger the lint, but could be out of bounds
    }

    public fun access_with_calculation() {
        let v = vector[1, 2, 3];
        let i = vector::length(&v) + 1;
        let _ = vector::borrow(&v, i); // This is out of bounds but might not be caught
    }

    public fun access_in_loop(limit: u64) {
        let v = vector[1, 2, 3];
        let i = 0;
        while (i < limit) { // limit could be larger than vector length
            let _ = vector::borrow(&v, i);
            i = i + 1;
        };
    }

    public fun access_with_complex_logic(a: u64, b: u64) {
        let v = vector[1, 2, 3];
        let i = if (a > b) { a - b } else { b - a };
        let _ = vector::borrow(&v, i); // This could be out of bounds depending on a and b
    }

    public fun access_after_conditional_pop(condition: bool) {
        let v = vector[1, 2, 3];
        if (condition) {
            vector::pop_back(&mut v);
        };
        let _ = vector::borrow(&v, 2); // This could be out of bounds if condition is true
    }

    public fun access_with_bitwise_operations(shift: u8) {
        let v = vector[1, 2, 3];
        let i = 1 << shift; // This could result in an out-of-bounds index
        let _ = vector::borrow(&v, i);
    }

    public fun access_with_dynamic_vector(n: u64) {
        let v = vector::empty<u64>();
        let i = 0;
        while (i < n) {
            vector::push_back(&mut v, i);
            i = i + 1;
        };
        let _ = vector::borrow(&v, n); // This is always out of bounds but might not be caught
    }
}
