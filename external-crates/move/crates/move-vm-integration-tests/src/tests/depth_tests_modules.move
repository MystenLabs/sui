module 0x7::A {
    public struct S has copy, drop {
        f1: 0x7::B::S,
        f2: 0x7::C::S,
    }

      public struct Box<T> has copy, drop, store { x: T }
      public struct Box3<T> has copy, drop, store { x: Box<Box<T>> }
      public struct Box7<T> has copy, drop, store { x: Box3<Box3<T>> }
      public struct Box15<T> has copy, drop, store { x: Box7<Box7<T>> }
      public struct Box31<T> has copy, drop, store { x: Box15<Box15<T>> }
      public struct Box63<T> has copy, drop, store { x: Box31<Box31<T>> }
      public struct Box127<T> has copy, drop, store { x: Box63<Box63<T>> }
}

module 0x7::B {
    public struct S has copy, drop {
        f1: u64,
        f2: u128,
    }
}
module 0x7::C {
    public struct S has copy, drop {
        f1: address,
        f2: bool,
    }
}

module 0x7::D {
    public struct S has copy, drop {
        f1: 0x7::B::S,
    }
}

module 0x7::E {
    public struct S<T> has copy, drop {
        f1: 0x7::F::S<T>,
        f2: u64,
    }
}

module 0x7::F {
    public struct S<T> has copy, drop {
        f1: T,
        f2: u64,
    }
}

module 0x7::G {
    public struct S<A, B> has copy, drop {
        f1: 0x7::H::S<B, A>,
        f2: u64,
    }
}

module 0x7::H {
    public struct S<A, B> has copy, drop {
        f1: 0x7::F::S<A>,
        f2: 0x7::E::S<B>,
        f3: 0x7::E::S<0x7::F::S<B>>,
        f4: A,
        f5: B,
        f6: u64,
    }
}

module 0x7::I {
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
