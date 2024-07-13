//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Option<T: drop> has drop {
        Some(T),
        None
    }

    fun t0(): u64 {
        match (Option::Some(0)) {
            Option::Some(x) => x,
            Option::None => 1,
        }
    }

    fun t1(opt: Option<Option<u64>>): u64 {
        match (opt) {
            Option::Some(Option::Some(x)) => x,
            Option::Some(Option::None) => 1,
            Option::None => 2,
        }
    }

    fun t2(opt: &Option<Option<u64>>, default: &u64): &u64 {
        match (opt) {
            Option::Some(Option::Some(x)) => x,
            Option::Some(Option::None) => default,
            Option::None => default,
        }
    }

    fun t3(opt: &mut Option<Option<u64>>, default: &mut u64): &mut u64 {
        match (opt) {
            Option::Some(Option::Some(x)) => x,
            Option::Some(Option::None) => default,
            Option::None => default,
        }
    }

    public fun run() {
        assert!(t0() == 0);

        assert!(t1(Option::Some(Option::Some(42))) == 42);
        assert!(t1(Option::Some(Option::None)) == 1);
        assert!(t1(Option::None) == 2);

        let x = 42;
        let y = 0;
        assert!(Option::Some(Option::Some(x)).t2(&y) == &x);
        assert!(Option::Some(Option::None).t2(&y) == &y);
        assert!(Option::None.t2(&y) == &y);

        let mut x = 42;
        let mut y = 0;
        assert!(Option::Some(Option::Some(x)).t3(&mut y) == &mut x);
        assert!(Option::Some(Option::None).t3(&mut y) == &mut y);
        assert!(Option::None.t3(&mut y) == &mut y);
    }
}

//# run 0x42::m::run
