module 0x42::M {
    // Incorrectly named constants
    const Another_BadName: u64 = 42; // Should trigger a warning
    const JSON_Max_Size: u64 = 1048576;
}
