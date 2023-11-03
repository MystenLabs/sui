module A::M {
    use B::N;

    #[test]
    fun nop() {
        N::nop()
    }
}
