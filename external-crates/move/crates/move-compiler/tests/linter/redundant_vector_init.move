module 0x42::loop_test {
    use std::vector;

    public fun vector_literals() {
        let a = vector::empty();
        vector::push_back(&mut a, 1u64);

        // intentionally ugly case
        let _ = vector::singleton({
            let x = 1u64;
            x
        });

        let _ = vector::singleton(1u64);
        let _ = vector[1u64];
    }

    #[allow(lint(verbose_vector_init))]
    public fun vector_literals_suppressed() {
        let a = vector::empty();
        vector::push_back(&mut a, 1u64);

        // intentionally ugly case
        let _ = vector::singleton({
            let x = 1u64;
            x
        });

        let _ = vector::singleton(1u64);
        let _ = vector[1u64];
    }
}
