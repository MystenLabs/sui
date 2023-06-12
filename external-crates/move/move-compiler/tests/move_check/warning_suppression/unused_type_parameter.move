#[allow(unused_type_parameter)]
module 0x42::m {
    struct S<T> { }
}

module 0x42::n {
    #[allow(unused_type_parameter)]
    struct S<T> { }
}
