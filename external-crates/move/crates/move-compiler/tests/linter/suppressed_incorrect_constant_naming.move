module 0x42::M {
    // Incorrectly named constants
    const Another_BadName: u64 = 42; // Should trigger a warning
    const YetAnotherName: u64 = 777; // Should not trigger a warning

    const minimumValue: u64 = 10; 
    const some_important_number: u64 = 55;
    
    const HttpTimeout: u64 = 30000; 
    const JSON_Max_Size: u64 = 1048576;
    const VALUE_MAX: u64 = 200;

    const numItems: u64 = 30;   
}
