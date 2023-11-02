module 0x1::Test {
    struct S<phantom K, phantom V> {
    }

    public fun f<K, V>(): S<K, V> {
        S {
        }
    }

    spec module {
        axiom<T> forall v1: T: true;
    }
}
