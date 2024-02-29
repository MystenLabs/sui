module 0x42::m {
    public enum X {
        A(u64)
    }

    public fun invalid(x: &mut X): u64 {
        match (x) {
            // mut is not needed here since we're matching on a mutable reference
            X::A(mut a) => {
                *a = *a + 1;
                *a
            }
        }
    }

    public fun valid(x: &mut X): u64 {
        match (x) {
            X::A(a) => {
                *a = *a + 1;
                *a
            }
        }
    }
}
