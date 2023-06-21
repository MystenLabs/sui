module 0x42::unused_functions {
    public fun f() {
        used_private()
    }

    // make sure that defining a function after its use does not matter
    fun unused_private() {}

    fun used_private() {}
}
