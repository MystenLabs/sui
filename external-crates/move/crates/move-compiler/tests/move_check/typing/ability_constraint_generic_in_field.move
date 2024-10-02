module 0x42::m {
    struct S<T: copy> { v: T }
    struct B<T> { v: S<T> }
}

module 0x42::n {
    struct A<T: copy> has copy { a: T }

    struct B<T> has copy {
        data: vector<A<T>>
    }
}
