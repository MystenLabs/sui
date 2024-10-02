module 0x42::k {
}

module 0x42::m {
    #[deprecated]
    friend 0x42::k;

    #[deprecated]
    use 0x42::k;
}
