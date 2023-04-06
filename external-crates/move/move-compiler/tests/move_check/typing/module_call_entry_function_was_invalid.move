address 0x2 {

// entry functions (previously public(script)) used to have visbility restrictions.
// These have been removed

module X {
    friend 0x2::M;
    fun f_private() {}
    public entry fun f_script() {}
    public(friend) fun f_friend() {}
}

module M {
    use 0x2::X;

    public entry fun f_script_call_script() { X::f_script() }

    fun f_private_call_script() { X::f_script() }
    public(friend) fun f_friend_call_script() { X::f_script() }
    public fun f_public_call_script() { X::f_script() }

    fun f_private_call_self_script() { f_script_call_script() }
    public(friend) fun f_friend_call_self_script() { f_script_call_script() }
    public fun f_public_call_self_script() { f_script_call_script() }

    public entry fun f_script_call_private() { X::f_private() }

    public entry fun f_script_call_friend() { X::f_friend() }
}

}
