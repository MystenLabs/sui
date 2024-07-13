//# init --edition 2024.beta

//# publish
module 0x42::m {
    public struct A { x: u64 }

    fun t00(s: A): u64 {
        match (s) {
            A { x: 0 } => 1,
            A { x } => x,
        }
    }

    fun t01(s: &A, default: &u64): &u64 {
        match (s) {
            A { x: 0 } => default,
            A { x } => x,
        }
    }

    fun t02(s: &mut A, default: &mut u64): &mut u64 {
        match (s) {
            A { x: 0 } => default,
            A { x } => x,
        }
    }

    public fun run() {
        let mut a = A { x: 42 };
        let mut b = A { x: 0 };
        let mut c = A { x: 2 };

        let d = &a;
        let e = &b;
        let f = &c;

        assert!(*d.t01(&1) == 42);
        assert!(*e.t01(&1) == 1);
        assert!(*f.t01(&1) == 2);

        assert!(*a.t02(&mut 1) == 42);
        assert!(*b.t02(&mut 1) == 1);
        assert!(*c.t02(&mut 1) == 2);

        assert!(a.t00() == 42);
        assert!(b.t00() == 1);
        assert!(c.t00() == 2);
    }
}


//# run 0x42::m::run
