#[allow(missing_phantom)]
module 0x42::m {
    struct B<phantom T> {}
    struct S<T> { f: B<T> }
}

module 0x42::n {
    struct B<phantom T> {}

    #[allow(missing_phantom)]
    struct S<T> { f: B<T> }
}
