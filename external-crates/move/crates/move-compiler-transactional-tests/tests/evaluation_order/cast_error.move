//# init --edition 2024.alpha

//# publish
#[allow(always_errors)]
module 0x42::m {
    const LARGE: u16 = 0xFFFF;

    public fun t(cond: bool): u16 {
        let x = LARGE as u8;
        if (!cond) return 0;
        x as u16
    }
}

//# run 0x42::m::t --args false
