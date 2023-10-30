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

    spec vector_choose_success {
        let post choice = choose i in 0..len(result) where result[i] == 1;
        ensures choice == 0 || choice == 2;
        ensures (choose min i in 0..len(result) where result[i] == 1) == 0;
    }

    #[test]
    public fun vector_choose_unsatisfied_predicate(): vector<u64> {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 1);
        vector::push_back(&mut v, 2);
        vector::push_back(&mut v, 1);
        v
    }

    spec vector_choose_unsatisfied_predicate {
        let post choice = choose i in 0..len(result) where result[i] == 3;
    }

    #[test]
    public fun vector_choose_min_unsatisfied_predicate(): vector<u64> {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 1);
        vector::push_back(&mut v, 2);
        vector::push_back(&mut v, 1);
        v
    }

    spec vector_choose_min_unsatisfied_predicate {
        let post choice_min = choose min i in 0..len(result) where result[i] == 3;
    }

    #[test]
    public fun simple_number_range_failure(): u64 { 1 }

    spec simple_number_range_failure {
        ensures result <= (choose x: u64 where x >= 4);
    }

    #[test]
    public fun simple_number_min_range_failure(): u64 { 1 }

    spec simple_number_min_range_failure {
        ensures result <= (choose min x: u64 where x >= 4);
    }
}
