module 0x42::false_negative_equal_operand {
   fun test_false_negatives() {
        // Complex expressions that are effectively the same but syntactically different
        let x = 10;
        let _ = (x + 0) == x;  // Semantically same operands but syntactically different
        let _ = (x * 1) == x;  // Semantically same operands but syntactically different

        // Function calls that return the same value
        let _ = get_constant() == get_constant();  // Same value but lint won't catch it
    }

    fun get_constant(): u64 { 42 }

    // Additional test for struct equality
    struct TestStruct has copy, drop { value: u64 }

    fun test_struct_operations() {
        let s = TestStruct { value: 10 };
        let _ = s == s;  // Should trigger lint
        
        let s2 = TestStruct { value: 10 };
        let _ = s == s2; // Should not trigger lint (different instances)
    }
}
