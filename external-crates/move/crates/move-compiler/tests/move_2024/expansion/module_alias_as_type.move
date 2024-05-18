module 0x2::X {}

module 0x2::M {
    use 0x2::X;
    fun foo(x: X) { x; }
}
