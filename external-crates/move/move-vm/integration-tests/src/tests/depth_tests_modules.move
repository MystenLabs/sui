address 0x2 {
module A {
    struct S has copy, drop {
        f1: 0x2::B::S,
        f2: 0x2::C::S,
    }

      struct Box<T> has copy, drop, store { x: T }
      struct Box3<T> has copy, drop, store { x: Box<Box<T>> }
      struct Box7<T> has copy, drop, store { x: Box3<Box3<T>> }
      struct Box15<T> has copy, drop, store { x: Box7<Box7<T>> }
      struct Box31<T> has copy, drop, store { x: Box15<Box15<T>> }
      struct Box63<T> has copy, drop, store { x: Box31<Box31<T>> }
      struct Box127<T> has copy, drop, store { x: Box63<Box63<T>> }
}

module B {
    struct S has copy, drop {
        f1: u64,
        f2: u128,
    }
}
module C {
    struct S has copy, drop {
        f1: address,
        f2: bool,
    }
}

module D {
    struct S has copy, drop {
        f1: 0x2::B::S,
    }
}

module E {
    struct S<T> has copy, drop {
        f1: 0x2::F::S<T>,
        f2: u64,
    }
}

module F {
    struct S<T> has copy, drop {
        f1: T,
        f2: u64,
    }
}

module G {
    struct S<A, B> has copy, drop {
        f1: 0x2::H::S<B, A>,
        f2: u64,
    }
}

module H {
    struct S<A, B> has copy, drop {
        f1: 0x2::F::S<A>,
        f2: 0x2::E::S<B>,
        f3: 0x2::E::S<0x2::F::S<B>>,
        f4: A,
        f5: B,
        f6: u64,
    }
}

module I {
    struct S<A, B> {
        f1: F<A>,
        f2: E<B>,
        f3: E<F<B>>,
        f4: E<F<F<B>>>,
        f5: E<F<F<LL<A, B>>>>,
        f6: A,
        f7: B,
        f8: u64,
    }

    struct E<T> {
        f1: F<T>,
        f2: u64,
    }

    struct F<T> {
        f1: T,
        f2: u64,
    }

    struct H<T> {
        f1: T,
        f2: u64,
    }

    struct G<phantom T> {
        f: H<u64>,
    }

    struct L<T> {
        g1: G<T>,
        g2: H<T>,
    }

    struct LL<phantom A, B> {
        g1: G<A>,
        g2: H<B>,
    }

    struct N<phantom Y> {
        f: u64
    }
}
}
