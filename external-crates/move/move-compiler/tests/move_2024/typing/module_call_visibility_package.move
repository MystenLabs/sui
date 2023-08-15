address 0x2 {

module X {
    public fun f_public() {}
}

module Y {
    public(package) fun f_package() {}
}

module M {
    use 0x2::X;
    use 0x2::Y;

    public fun f_public() {}
    public(package) fun f_package() {}
    fun f_private() {}

    // a public(fpackage) fun can call public funs in another module
    public(package) fun f_package_call_public() { X::f_public() }

    // a public(package) fun can call private and public funs defined in its own module
    public(package) fun f_package_call_self_private() { Self::f_private() }
    public(package) fun f_package_call_self_public() { Self::f_public() }

    // a public functions can call a public(package) function defined in the same module
    // as well as package functions defined in other modules in the same package
    public fun f_public_call_package() { Y::f_package() }
    public fun f_public_call_self_package() { Self::f_package() }

    // a public(package) functions can call a public(package) function defined in the same module
    // as well as package functions defined in other modules in the same package
    public(package) fun f_package_call_package() { Y::f_package() }
    public(package) fun f_package_call_self_package() { Self::f_package() }

    // a private functions can call a public(package) function defined in the same module
    // as well as package functions defined in other modules in the same package
    fun f_private_call_package() { Y::f_package() }
    fun f_private_call_self_package() { Self::f_package() }
}

}
