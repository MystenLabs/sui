module 0x2::TestViz {
    /// This is a public function
    public fun this_is_a_public_fun() { }

    // /// This is a public friend function
    // public(friend) fun this_is_a_public_friend_fun() {}

    /// This is a public entry function
    public entry fun this_is_a_public_script_fun() {}

    #[allow(unused_function)]
    /// This is a private function
    fun this_is_a_private_fun() {}
}
