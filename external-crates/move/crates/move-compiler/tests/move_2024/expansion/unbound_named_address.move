// Unbound address in all cases
module A::M { // suggests declaration
    use B::X;

    friend C::M;
    friend D::M::foo;

    struct S {
        x: E::M::S,
    }

    fun foo() {
        let x = F::M::S {}; x;
        G::M::foo();
        let c = H::M::C; c;
        let a = @I; a; // suggests declaration
    }
}
