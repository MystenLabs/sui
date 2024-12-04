module 0x42::excessive_nesting_true_positive {

    #[allow(unused_assignment)]
    fun test_true_positive_direct_nesting() {
        let value = 100;
        
        // Direct nesting that should be combined with &&
        if (value > 50) {
            if (value < 150) {  // Should trigger warning
                if (value != 100) {  // Should trigger another warning
                    value = 75;
                };
            };
        };

    }

    #[allow(unused_assignment)]
    fun test_true_positive_complex_nesting() {
        let value = 100;
        if (value > 0) {
            let temp = value + 50;
            if (temp > 100) {  // Should trigger warning - indirect nesting
                if (value < 200) {  // Should trigger another warning
                    value = temp;
                };
            };
        };
    }
}
