module prettier::simple_function {
    fun say_hello() {}

    /// This is a simple function
    fun simple_function(): u64 {
        say_hello();
        100
    }

    fun breakable_parameters(
        first_breakable_parameter: u64,
        second_breakable_parameter: u64,
        third_breakable_parameter: u64,
        fourth_breakable_parameter: u64,
        fifth_breakable_parameter: u64,
    ): u64 {
        first_breakable_parameter
            + second_breakable_parameter
            + third_breakable_parameter
            + fourth_breakable_parameter
            + fifth_breakable_parameter
    }

    entry fun private_entry() {}

    public fun public_function() {}

    public entry fun public_entry() {}

    public(package) fun public_package_function() {}

    public(package) entry fun public_package_entry() {}
}
