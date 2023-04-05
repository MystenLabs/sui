module 0x42::loop_inv {
    use std::vector;

    fun f() {
        let v1 = vector::empty<u64>();
        let v2 = vector::empty<u64>();
        let index = 0;
        let i = 0;

        while ({
            spec {
                invariant index == len(v1);
            };
            i < 10000
        }) {
            i = i + 1;
            if (i == 100) {
                continue
            };
            vector::push_back(&mut v1, index);
            vector::push_back(&mut v2, index);
            index = index + 1;
        };


        spec {
            assert index == len(v1);
        };
    }
}
