module 0x42::m {

    public struct Box { value: u64 }

    public fun test(b: &mut Box) {
        match (b) {
            Box { value } => {
                *value = *value + 1;
            },
            _ => (),
        }
    }

}
