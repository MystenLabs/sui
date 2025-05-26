module 0x42::swap_sequence_tests {

    #[allow(unused_assignment)]
    public fun test_valid_assignments() {
        let a = 10;
        let b = 20;
        let c = 30;
        
        // Valid sequence of different assignments
        a = b + c;
        b = c * 2;
        c = a - b;
    }

    #[allow(unused_assignment)]
    public fun test_valid_variable_reuse() {
        let x = 100;
        let y = 200;
        
        // Valid reuse of variables without forming a swap pattern
        x = x + y;
        y = x * 2;
    }

    #[allow(unused_assignment)]
    public fun test_valid_complex_assignments() {
        let value1 = 5;
        let value2 = 10;
        let result = 0;
        
        // Complex assignments that shouldn't trigger the linter
        result = value1 + value2;
        value1 = result * 2;
        value2 = value1 / 2;
    }
}
