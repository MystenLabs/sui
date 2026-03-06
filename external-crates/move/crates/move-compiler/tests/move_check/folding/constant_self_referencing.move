module 0x42::m {
    // Self-referencing constant
    const C: u64 = C + 1;

    // Another self-referencing constant
    const D: u64 = D;

    // Constant that depends on a self-referencing constant
    const E: u64 = C + 2;

    // Non-cyclic constant (should be fine)
    const F: u64 = 5;

    // Constant that depends on both a self-referencing and a valid constant
    const G: u64 = C + F;
}
