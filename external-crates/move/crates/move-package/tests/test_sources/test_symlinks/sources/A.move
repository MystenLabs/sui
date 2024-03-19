module A::A {
    use A::M;
    public fun foo() {
        M::foo()
    }
}
