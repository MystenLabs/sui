#[allow(unused_assignment)]
module 0x42::swap_sequence_tests {

    #[allow(lint(almost_swapped))]
    public fun test_direct_swap() {
        let a = 10u64;
        let b = 20u64;
        
        // Unnecessary swap sequence
        let temp = b;
        b = a;
        a = temp;
    }

    #[allow(lint(almost_swapped))]
    public fun test_direct_swap_suppress_1() {
        let a = 10u64;
        let b = 20u64;
        
        {
            let temp = b;
            b = a;
            a = temp;
        }
    }

    #[allow(lint(almost_swapped))]
    public fun test_direct_swap_suppress_2() {
        let a = 10u64;
        let b = 20u64;
        
        {
            let temp = b;
            b = a;
            a = temp;
        }
    }

    #[allow(lint(almost_swapped))]
    public fun test_direct_swap_suppress_3() {
        let a = 10u64;
        let b = 20u64;
        
        let temp = b;
        b = a;
        a = temp;
    }
}
