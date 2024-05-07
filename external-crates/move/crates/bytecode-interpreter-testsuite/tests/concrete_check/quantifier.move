module 0x2::A {
    use std::vector;

   #[test]
    public fun init_vector_success(): vector<u64> {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 1);
        vector::push_back(&mut v, 2);
        v
    }

    #[test]
    public fun init_vector_failure(): vector<u64> {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 1);
        vector::push_back(&mut v, 2);
        v
    }

}
