#[evm_contract] // for passing evm test flavor
module 0x1::Foo {
    use 0x1::Bar;

    public foo() {
        Bar::bar();
    }
}
