module 0x42::mod_0 {
    public fun f_public() {}
}

module 0x42::mod_1 {
    public(package) fun f_package() {}
}

module 0x42::mod_2 {
    use 0x42::mod_0;
    use 0x42::mod_1;

    public fun f_public() {}
    public(package) fun f_package() {}
    fun f_private() {}

    // a public(fpackage) fun can call public funs in another module
    public(package) fun f_package_call_public() { mod_0::f_public() }

    // a public(package) fun can call private and public funs defined in its own module
    public(package) fun f_package_call_self_private() { Self::f_private() }
    public(package) fun f_package_call_self_public() { Self::f_public() }

    // a public functions can call a public(package) function defined in the same module
    // as well as package functions defined in other modules in the same package
    public fun f_public_call_package() { mod_1::f_package() }
    public fun f_public_call_self_package() { Self::f_package() }

    // a public(package) functions can call a public(package) function defined in the same module
    // as well as package functions defined in other modules in the same package
    public(package) fun f_package_call_package() { mod_1::f_package() }
    public(package) fun f_package_call_self_package() { Self::f_package() }

    // a private functions can call a public(package) function defined in the same module
    // as well as package functions defined in other modules in the same package
    fun f_private_call_package() { mod_1::f_package() }
    fun f_private_call_self_package() { Self::f_package() }
}
