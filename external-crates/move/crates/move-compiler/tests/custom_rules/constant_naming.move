module 0x42::M {
    // Correctly named constants
    const MAX_LIMIT: u64 = 1000;
    const MIN_THRESHOLD: u64 = 10;
    const MIN_U64: u64 = 10;

    // // Incorrectly named constants
    const Maxcount: u64 = 500; // Should trigger a warning
    const MinValue: u64 = 1; // Should trigger a warning
    const Another_badName: u64 = 42; // Should trigger a warning
    const YetAnotherName: u64 = 777; // Should trigger a warning
}