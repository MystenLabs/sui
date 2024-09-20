module 0x42::m {
    struct S<T: copy + drop> has copy, drop { x: T }

    public fun test<T: copy + drop>(s: S<S<T>>)  {
        let _x = &*&*&s.x.x;
    }
}
