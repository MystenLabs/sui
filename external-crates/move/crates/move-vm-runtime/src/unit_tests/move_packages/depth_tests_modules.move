module 0x2::A {
    public struct S has copy, drop {
        f1: 0x2::B::S,
        f2: 0x2::C::S,
    }

      public struct Box<T> has copy, drop, store { x: T }
      public struct Box3<T> has copy, drop, store { x: Box<Box<T>> }
      public struct Box7<T> has copy, drop, store { x: Box3<Box3<T>> }
      public struct Box15<T> has copy, drop, store { x: Box7<Box7<T>> }
      public struct Box31<T> has copy, drop, store { x: Box15<Box15<T>> }
      public struct Box63<T> has copy, drop, store { x: Box31<Box31<T>> }
      public struct Box127<T> has copy, drop, store { x: Box63<Box63<T>> }
}

module 0x2::B {
    public struct S has copy, drop {
        f1: u64,
        f2: u128,
    }
}
module 0x2::C {
    public struct S has copy, drop {
        f1: address,
        f2: bool,
    }
}

module 0x2::D {
    public struct S has copy, drop {
        f1: 0x2::B::S,
    }
}

module 0x2::E {
    public struct S<T> has copy, drop {
        f1: 0x2::F::S<T>,
        f2: u64,
    }
}

module 0x2::J {
    // Generic enum with two variants: exercises `extra_layout_nodes` (one per variant) and
    // parameter folding across variants.
    public enum E<T> has copy, drop {
        A(T),
        B(T, u64),
    }
}

module 0x2::F {
    public struct S<T> has copy, drop {
        f1: T,
        f2: u64,
    }
}

module 0x2::G {
    public struct S<A, B> has copy, drop {
        f1: 0x2::H::S<B, A>,
        f2: u64,
    }
}

module 0x2::H {
    public struct S<A, B> has copy, drop {
        f1: 0x2::F::S<A>,
        f2: 0x2::E::S<B>,
        f3: 0x2::E::S<0x2::F::S<B>>,
        f4: A,
        f5: B,
        f6: u64,
    }
}

module 0x2::I {
    public struct S<A, B> {
        f1: F<A>,
        f2: E<B>,
        f3: E<F<B>>,
        f4: E<F<F<B>>>,
        f5: E<F<F<LL<A, B>>>>,
        f6: A,
        f7: B,
        f8: u64,
    }

    public struct E<T> {
        f1: F<T>,
        f2: u64,
    }

    public struct F<T> {
        f1: T,
        f2: u64,
    }

    public struct H<T> {
        f1: T,
        f2: u64,
    }

    public struct G<phantom T> {
        f: H<u64>,
    }

    public struct L<T> {
        g1: G<T>,
        g2: H<T>,
    }

    public struct LL<phantom A, B> {
        g1: G<A>,
        g2: H<B>,
    }

    public struct N<phantom Y> {
        f: u64
    }
}

module 0x2::P {
    public struct Four<phantom A, phantom B, phantom C, phantom D> has copy, drop, store {
        value: u8,
    }

    public struct Chain0<phantom T> has copy, drop, store { value: u8 }
    public struct Chain1<phantom T> has copy, drop, store { value: Chain0<Four<T, T, T, T>> }
    public struct Chain2<phantom T> has copy, drop, store { value: Chain1<Four<T, T, T, T>> }
    public struct Chain3<phantom T> has copy, drop, store { value: Chain2<Four<T, T, T, T>> }
    public struct Chain4<phantom T> has copy, drop, store { value: Chain3<Four<T, T, T, T>> }

    public struct Linear0<phantom T> has copy, drop, store { value: u8 }
    public struct Linear1<phantom T> has copy, drop, store { value: Linear0<T> }
    public struct Linear2<phantom T> has copy, drop, store { value: Linear1<T> }
    public struct Linear3<phantom T> has copy, drop, store { value: Linear2<T> }
    public struct Linear4<phantom T> has copy, drop, store { value: Linear3<T> }
}

module 0x2::W {
    // A family with nested datatype applications in field position, exercising the linearized
    // size-formula path: subterm vs. field-root applications, vector layers on arguments, and
    // term merging across fields (value depths max, layout sizes sum).
    public struct SS<A> {
        x: vector<A>,
    }

    public struct Z {
        z: u64,
    }

    public struct KK<X, Y> {
        a: X,
        b: Y,
    }

    public struct T<P, Q> {
        p: P,
        q: vector<Q>,
    }

    public struct W<A> {
        f: T<SS<A>, vector<KK<u64, Z>>>,
        g: SS<A>,
        h: SS<A>,
    }
}
