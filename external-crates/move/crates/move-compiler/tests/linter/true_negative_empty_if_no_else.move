module 0x42::empty_if_no_else_true_negative {

    #[allow(unused_variable)]
    public fun test_if_with_content(x: u64) {
        if (x > 10) {
            let y = x + 1;
        
        }
    }

    public fun test_if_else(x: u64) {
        if (x > 5) {
            // Do something
        } else {
            // Do something else
        };
    }

    public fun test_if_else_if(x: u64) {
        if (x > 10) {
            // Do something
        } else if (x > 5) {
            // Do something else
        };
    }
}
