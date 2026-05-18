module 0x2a::M {
    const PHASE_INIT: u8 = 0;
    const PHASE_HALT: u8 = 255;

    public fun next(phase: &u8): u8 {
        match (phase) {
            PHASE_INIT => 1,
            PHASE_HALT => PHASE_HALT,
            p @ _ if (*p < 128) => *p + 1,
            _ => PHASE_HALT,
        }
    }
}
