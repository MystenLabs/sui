module dep_on_upgrading_package::my_module {
    use base_addr::base;

    public fun call_return_0(): u64 { base::return_0() }
}
