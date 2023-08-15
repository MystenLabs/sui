address 0x2 {

module X {
    public(package) fun foo(): u64 { 0 }
}

module Y {
    friend 0x2::M;
    public(friend) fun foo(): u64 { 0 }
}

module M {
    use 0x2::X;
    fun bar(): u64 { X::foo() }
}

}
