module 0x42::t {
    const C_ZERO: u64 = 0;
}

module 0x42::d {
    use 0x42::t::C_ZERO;
    const D: u64 = C_ZERO + 1;
}
