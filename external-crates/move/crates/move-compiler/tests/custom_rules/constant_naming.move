module 0x42::M {
    // Correctly named constants
    const MAX_LIMIT: u64 = 1000;
    const MIN_THRESHOLD: u64 = 10;
    const MIN_U64: u64 = 10;

    // // Incorrectly named constants
    const Maxcount: u64 = 500; // Should not trigger a warning
    const MinValue: u64 = 1; // Should not trigger a warning
    const Another_BadName: u64 = 42; // Should trigger a warning
    const YetAnotherName: u64 = 777; // Should not trigger a warning

    const minimumValue: u64 = 10; 
    const some_important_number: u64 = 55;
    
    const HttpTimeout: u64 = 30000; 
    const JSON_Max_Size: u64 = 1048576;
    const VALUE_MAX: u64 = 200;
    const numItems: u64 = 30;   
}