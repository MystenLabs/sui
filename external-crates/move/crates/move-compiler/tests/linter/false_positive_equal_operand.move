module 0x42::false_positive_equal_operand {
    fun test_false_positives() {
        // Iterator-like patterns where comparing same variable is intentional
        let mut_ref = &mut 0;
        while *mut_ref != *mut_ref {  // Legitimate use: checking for changes in mutable reference
            break
        };

        // Checking for NaN (Not a Number) in floating point implementations
        let nan_check = is_nan(1.0);  // Simulated floating point check
        
        // Checking for pointer/reference equality (if Move had raw pointers)
        let obj = object();
        let _ = reference_equals(obj, obj);  // Legitimate use: checking if references point to same object

        // Checking for monotonicity
        let x = 10;
        let y = 20;
        assert!(x <= x && x <= y, 0); // Legitimate use in monotonicity checks
    }

    // Helper functions for false positive cases
    fun is_nan(_x: u64): bool { 
        false  // Simulated NaN check
    }

    fun object(): &vector<u8> {
        &vector[1, 2, 3]
    }

    fun reference_equals<T>(a: &T, b: &T): bool {
        // Simulated reference equality check
        std::hash::sha2_256(a) == std::hash::sha2_256(b)
    }
}
