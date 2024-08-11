module 0x42::m {
    #[deprecated]
    const A: u64 = 1;

    #[deprecated(note = b"use D instead")]
    const B: u64 = 2;

    #[deprecated(note = b"You should use E instead")]
    const C: u64 = 3;

    const D: u64 = 4;
    const E: u64 = 5;

    const Combo: u64 = {
        A + B + C + D + E + B
    };
}
