module 0x42::swap_sequence_tests {

    #[allow(unused_assignment)]
    public fun test_direct_swap() {
        let a = 10;
        let b = 20;
        
        // Unnecessary swap sequence
        let temp = b;
        b = a;
        a = temp;
    }

    #[allow(unused_assignment)]
    public fun test_indirect_swap() {
        let x = 100;
        let y = 200;
        
        // Unnecessary indirect swap using temporary variable
        let temp = x;
        x = y;
        y = temp;
    }

    #[allow(unused_assignment)]
    public fun test_multiple_swaps() {
        let a = 1;
        let b = 2;
        let c = 3;
        
        // Series of unnecessary swaps
        let temp = a;
        a = b;
        b = temp;
        
        temp = b;
        b = c;
        c = temp;
    }
}
