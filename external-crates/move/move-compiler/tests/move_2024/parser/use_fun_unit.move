module a::m {
    use fun foo as ().foo;
    fun foo() { abort 0 }
}
