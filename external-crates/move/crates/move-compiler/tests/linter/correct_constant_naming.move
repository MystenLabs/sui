module 0x42::M {
    // Correctly named constants
    const MAX_LIMIT: u64 = 1000;
    const MIN_THRESHOLD: u64 = 10;
    const MIN_U64: u64 = 10;
    const Maxcount: u64 = 500; // Should not trigger a warning
    const MinValue: u64 = 1; // Should not trigger a warning
}
