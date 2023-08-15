address 0x2 {

module X {
    friend 0x2::Y;
    friend 0x2::M;
    friend 0x2::P;
    public(friend) fun f_pkg_friend(): u64 { 0 }
}

module Y {
    use 0x2::X;
    public(package) fun f_pkg_public(): u64 { X::f_pkg_friend() }
}

module M {
    use 0x2::X;
    use 0x2::Y;

    // a fun can call a public(friend) fun
    public(package) fun f_package_call_friend(): u64 { X::f_pkg_friend() }
    public fun f_public_call_friend(): u64 { X::f_pkg_friend() }
    fun f_private_call_friend(): u64 { X::f_pkg_friend() }

    // a fun can call a public(package) fun
    public(package) fun f_package_call_package(): u64 { Y::f_pkg_public() }
    public fun f_public_call_package(): u64 { Y::f_pkg_public() }
    fun f_private_call_package(): u64 { Y::f_pkg_public() }
}

module P {
    use 0x2::X;
    use 0x2::Y;

    // a fun can call a public(friend) fun
    public(friend) fun f_friend_call_friend(): u64 { X::f_pkg_friend() }
    public fun f_public_call_friend(): u64 { X::f_pkg_friend() }
    fun f_private_call_friend(): u64 { X::f_pkg_friend() }

    // a fun can call a public(package) fun
    public(friend) fun f_friend_call_package(): u64 { Y::f_pkg_public() }
    public fun f_public_call_package(): u64 { Y::f_pkg_public() }
    fun f_private_call_package(): u64 { Y::f_pkg_public() }
}

}
