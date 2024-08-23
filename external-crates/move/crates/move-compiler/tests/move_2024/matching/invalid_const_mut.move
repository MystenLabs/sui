module 0x42::m {

    const ZERO: u64 = 0;

    public struct Box { value: u64 }

    public fun test(b: Box): u64 {
        match (b) {
            Box { value: mut ZERO } => ZERO,
            _ => 10,
        }
    }

    public fun test(b: &Box): u64 {
        match (b) {
            Box { value: mut ZERO } => ZERO,
            _ => 10,
        }
    }

    public fun test(b: &mut Box): u64 {
        match (b) {
            Box { value: mut ZERO } => ZERO,
            _ => 10,
        }
    }

}
