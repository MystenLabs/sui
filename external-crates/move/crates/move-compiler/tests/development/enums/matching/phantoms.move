module 0x42::a {

    public struct X { x: u64 }

    public struct Y<phantom T> { y: u64 }

    public enum P<phantom X> {
        A,
        B(Y<X>),
        C
    }

    public enum Q<phantom X> {
        A,
        B { x: Y<X> },
        C
    }

    public enum R<phantom X> {
        A(Y<X>),
        B { x: Y<X> },
        C
    }

    public enum S<phantom X> {
        A,
        B,
        C
    }

    public enum T<phantom X> {
        A(S<X>),
        B { x: S<X> },
        C
    }

    public enum U<phantom X> {
        A(Y<X>),
        B { x: Y<X> },
        C
    }

    public enum V<phantom X> {
        A(U<X>),
        B { x: U<X> },
        C
    }

}
