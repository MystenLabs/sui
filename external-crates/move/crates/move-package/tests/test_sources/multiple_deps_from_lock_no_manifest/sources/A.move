module A::A {
    use D::D as D;
    use C::C as C;

    public fun foo() {
        D::foo();
        C::foo()
    }
}
