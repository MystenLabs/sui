// Two constants with the same name at different addresses are fine
module 0x42::m {
    public(package) const C: u64 = 1;
}

module 0x43::m {
    public(package) const C: u64 = 2;
}

module 0x42::ex {
    public fun collision(): u64 {
        0x42::m::C + 0x43::m::C
    }
}
