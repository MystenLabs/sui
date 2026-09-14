module my_package::m {
    const ZERO: address = @my_package;
    const NUMERICAL_ZERO: address = @0x0;
    const NON_ZERO_NAMED: address = @M;

    fun zero(): address {
        @my_package
    }
}
