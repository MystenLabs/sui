module 0x42::mod_0 {
    fun f_private() {}
    public(package) fun f_pkg_public() {}
}

module 0x54::mod_1 {
    use 0x42::mod_0;

    // a fun cannot call a package funs at another address
    public(package) fun f_package_call_package() { mod_0::f_pkg_public() }
    public fun f_public_call_package() { mod_0::f_pkg_public() }
    fun f_private_call_package() { mod_0::f_pkg_public() }
}
