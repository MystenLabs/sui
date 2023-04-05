#[evm_contract] // for passing evm test flavor
module A::A {
    use A::Foo;

    fun f(): u64 {
        Foo::foo()
    }
}
