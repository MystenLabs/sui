
module 0x42::circular {
    const C0: u64 = C1 + 1;
    const C1: u64 = C2 + 1;
    const C2: u64 = C0 + 1;
    const C3: u64 = C1 + 1;
    const C4: u64 = 4;
    const C5: u64 = C1 + C4;
}
