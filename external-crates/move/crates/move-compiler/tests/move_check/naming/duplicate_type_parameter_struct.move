module 0x8675309::M {
    struct S<T, T> { f: T }
    struct S2<T: drop, T: key, T> { f: T }
    struct R<T, T> { f: T }
    struct R2<T: drop, T: key, T> { f: T }
}
