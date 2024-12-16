module 0x42::M {

    #[allow(lint(unnecessary_unit))]
    fun test_true_positives() {
        let x = 5;
        
        // Should suggest: x <= 10
        if (x == 10 || x < 10) {};
        
        // Should suggest: x >= 20
        if (x == 20 || x > 20) {};
        
        // Should suggest: x not in [10..20]
        if (x < 10 || x > 20) {};
        
        // Should suggest: x not in (10..20)
        if (x <= 10 || x >= 20) {};
        
        // Variations with swapped operands
        if (x < 10 || x == 10) {};  // Should suggest: x <= 10
        if (x > 20 || x == 20) {};  // Should suggest: x >= 20
        if (x > 20 || x < 10) {};   // Should suggest: x not in [10..20]
        if (x >= 20 || x <= 10) {}; // Should suggest: x not in (10..20)
    }
}
