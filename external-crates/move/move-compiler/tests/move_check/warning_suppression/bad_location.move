// tests an unsupported location for the attribute for warning supression

module 0x42::m {
    #[allow(unused)]
    use 0x42::x;

    #[allow(all)]
    friend 0x42::a;
}

module 0x42::x {}

module 0x42::a {}
