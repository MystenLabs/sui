address 0x2 {

// no visibility restructions around entry


module X {
    public fun f_public() {}
    public entry fun f_script() {}
}

module Y {
    friend 0x2::M;
    public(friend) fun f_friend() {}
}

module M {
    use 0x2::X;
    use 0x2::Y;

    public fun f_public() {}
    public(friend) fun f_friend() {}
    public entry fun f_script() {}
    fun f_private() {}

    public entry fun f_script_call_script() { X::f_script() }

    public entry fun f_script_call_friend() { Y::f_friend() }

    public entry fun f_script_call_public() { X::f_public() }

    public entry fun f_script_call_self_private() { Self::f_private() }
    public entry fun f_script_call_self_public() { Self::f_public() }
    public entry fun f_script_call_self_friend() { Self::f_friend() }
    public entry fun f_script_call_self_script() { Self::f_script() }
}

}
