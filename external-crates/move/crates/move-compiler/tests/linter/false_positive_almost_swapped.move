module 0x42::swap_sequence_tests {

    #[allow(unused_assignment)]
    public fun test_legitimate_value_exchange() {
        let balance1 = 1000;
        let balance2 = 2000;
        
        // Legitimate exchange of values that might trigger the linter
        let previous_balance = balance1;
        balance1 = balance2;
        // Some business logic here
        balance2 = previous_balance;
    }

    #[allow(unused_assignment)]
    public fun test_temporary_backup() {
        let current_value = 100;
        let new_value = 200;
        
        // Legitimate backup and update pattern
        let backup = current_value;
        current_value = new_value;
        // Some validation or computation
        if (current_value > 150) {
            current_value = backup;
        };
    }
}
