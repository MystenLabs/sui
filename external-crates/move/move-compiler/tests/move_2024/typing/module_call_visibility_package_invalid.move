address 0x2 {

module X {
    fun f_private() {}
    public(package) fun f_pkg_public() {}
}

}

address 0x4 {

module A {
    use 0x2::X;

    // a fun cannot call a package funs at another address
    public(package) fun f_package_call_package() { X::f_pkg_public() }
    public fun f_public_call_package() { X::f_pkg_public() }
    fun f_private_call_package() { X::f_pkg_public() }
}

module B {
    use 0x2::X;

    // a fun cannot call a package funs at another address
    public(friend) fun f_friend_call_package() { X::f_pkg_public() }
    public fun f_public_call_package() { X::f_pkg_public() }
    fun f_private_call_package() { X::f_pkg_public() }
}



}
