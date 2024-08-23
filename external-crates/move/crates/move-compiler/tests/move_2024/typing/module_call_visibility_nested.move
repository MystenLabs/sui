module 0x42::mod_0 {
    public(package) fun f_pkg_public(): u64 { 0 }
}

module 0x42::mod_1 {
    use 0x42::mod_0;
    public(package) fun f_pkg_public(): u64 { mod_0::f_pkg_public() }
}

module 0x42::mod_2 {
    use 0x42::mod_0;
    use 0x42::mod_1;

    // a fun can call a public(friend) fun
    public(package) fun f_package_call_friend(): u64 { mod_0::f_pkg_friend() }
    public fun f_public_call_friend(): u64 { mod_0::f_pkg_friend() }
    fun f_private_call_friend(): u64 { mod_0::f_pkg_friend() }

    // a fun can call a public(package) fun
    public(package) fun f_package_call_package(): u64 { mod_1::f_pkg_public() }
    public fun f_public_call_package(): u64 { mod_1::f_pkg_public() }
    fun f_private_call_package(): u64 { mod_1::f_pkg_public() }
}
