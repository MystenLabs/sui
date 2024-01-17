//# init --edition 2024.alpha

//# publish
module 0x42::D {
    public enum T {
        A,
    }

    public fun foo(): T {
        T::A
    }
}

//# publish
module 0x42::C {
    public enum T has drop {
        A,
    }

    public fun foo(): T {
        T::A
    }
}

//# publish
// names used to try to force an ordering of depedencies
module 0x42::B {
    public fun foo(): (0x42::C::T, 0x42::D::T) {
        (0x42::C::foo(), 0x42::D::foo())
    }
}

//# publish
module 0x42::A {
    public struct T {
        t_c: 0x42::C::T,
        t_d: 0x42::D::T,
    }
    public fun foo(): T {
        let (t_c, t_d) = 0x42::B::foo();
        T { t_c, t_d }
    }
}
