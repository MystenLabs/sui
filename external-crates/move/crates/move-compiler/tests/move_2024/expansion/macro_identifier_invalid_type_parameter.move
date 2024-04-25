module a::m {
    // invalid type parameter
    macro fun foo<$_, $__, $_t>() {}
}
