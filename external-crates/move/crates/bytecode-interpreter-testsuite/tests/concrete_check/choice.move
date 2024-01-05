module 0x2::A {
    use std::vector;

    #[test]
    public fun vector_choose_success(): vector<u64> {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 1);
        vector::push_back(&mut v, 2);
        vector::push_back(&mut v, 1);
        v
    }

    #[test]
    public fun vector_choose_unsatisfied_predicate(): vector<u64> {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 1);
        vector::push_back(&mut v, 2);
        vector::push_back(&mut v, 1);
        v
    }

    #[test]
    public fun vector_choose_min_unsatisfied_predicate(): vector<u64> {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 1);
        vector::push_back(&mut v, 2);
        vector::push_back(&mut v, 1);
        v
    }

    #[test]
    public fun simple_number_range_failure(): u64 { 1 }

    #[test]
    public fun simple_number_min_range_failure(): u64 { 1 }

}
