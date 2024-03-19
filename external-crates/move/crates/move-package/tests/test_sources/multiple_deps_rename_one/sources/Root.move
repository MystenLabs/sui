module A::A {
    use DA::A as DAA;
    use C::A as CA;

    public fun foo() {
        DAA::foo();
        CA::foo()
    }
}
