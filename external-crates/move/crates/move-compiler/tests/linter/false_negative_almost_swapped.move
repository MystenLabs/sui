module 0x42::swap_sequence_tests {

    #[allow(unused_assignment)]
    public fun test_complex_swap() {
        let a = 10;
        let b = 20;
        
        // Complex swap pattern that might be missed
        let temp1 = a + 5;
        let temp2 = b - 5;
        a = temp2;
        b = temp1;
    }

    #[allow(unused_assignment)]
    public fun test_conditional_swap() {
        let x = 100;
        let y = 200;
        let condition = true;
        
        // Conditional swap that might be missed
        if (condition) {
            let temp = x;
            x = y;
            y = temp;
        };
    }
}
