module 0x42::a {

    public struct X {}

    public struct Y<T> { t: T }

    public enum Q<phantom X> {
        A,
        B(X),
        C
    }

    public enum R<phantom X> {
        A,
        B { x: X },
        C
    }

    public enum S<phantom X> {
        A(X),
        B { x: X },
        C
    }

    public enum T<phantom X> {
        A,
        B(Y<X>),
        C
    }

    public enum U<phantom X> {
        A,
        B { x: Y<X> },
        C
    }

    public enum V<phantom X> {
        A(X, Y<X>),
        B { x: X, y: Y<X> },
        C
    }

    public enum W<phantom X> {
        A(X, V<X>),
        B { v: V<X> , x: X},
        C
    }

}
