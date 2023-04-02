module 0x2::A {
    use std::vector;

    struct R has key { i: u64 }

    struct S has key { i: u64 }

    #[test(a=@0x2)]
    public fun move_to_test(a: &signer) {
        let r = R {i: 1};
        let s = S {i: 1};
        move_to(a, r);
        move_to(a, s);

        spec {
            assert forall a: address where exists<R>(a): exists<S>(a);
        };
    }

    #[test]
    public fun init_vector_success(): vector<u64> {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 1);
        vector::push_back(&mut v, 2);
        v
    }

    spec init_vector_success {
        ensures forall num in result: num > 0;
        ensures exists num in result: num == 2;
        ensures exists i in 0..len(result): result[i] == 1;
    }

    #[test]
    public fun init_vector_failure(): vector<u64> {
        let v = vector::empty<u64>();
        vector::push_back(&mut v, 1);
        vector::push_back(&mut v, 2);
        v
    }

    spec init_vector_failure {
        ensures exists i: u64 where (i == len(result) - 1): result[i] == 1;
        ensures exists i: u64: (i == len(result) - 1) ==> result[i] == 1;
        ensures exists i: u64: result[i] == 1;
    }
}
