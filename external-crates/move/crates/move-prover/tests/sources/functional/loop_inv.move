module 0x42::loop_inv {
    use std::vector;

    fun g() {
        let v = vector::empty<u64>();
        let i = 0;
        while (i < 5) {
            vector::push_back(&mut v, 0);
        };
        spec {
            assert true;
        };
    }
}
